//! Pixel-data decode + CPU windowing.
//!
//! `load_raw` decodes a DICOM file once into rescaled f32 values plus
//! metadata about default window and photometric inversion. `render_rgba`
//! then turns those values into RGBA8 with a window/level applied — fast
//! enough to call on every drag delta for interactive W/L.

use anyhow::{Context, Result};
use dicom::object::open_file;
use dicom_pixeldata::{
    ConvertOptions, ModalityLutOption, PhotometricInterpretation, PixelDecoder, VoiLutOption,
};
use std::path::Path;

/// Cap interactive display at this many pixels per side. Anything larger is
/// decimated at load time so window/level dragging stays smooth on MG-sized
/// images. Non-diagnostic viewer — full-resolution display is not a goal.
const MAX_DISPLAY_DIM: u32 = 2048;

/// Which VOI window to apply when calling [`decode_to_rgba`].
#[derive(Debug, Clone, Copy)]
pub enum WindowSetting {
    /// Use the file's default window if present, otherwise full range.
    Auto,
    /// Caller-supplied window centre / width.
    Manual {
        /// Window centre in rescaled units (HU on CT).
        center: f64,
        /// Window width in rescaled units.
        width: f64,
    },
}

/// RGBA8 buffer returned by [`decode_to_rgba`].
pub struct DecodedFrame {
    /// Frame width in pixels (after decimation, if any).
    pub width: u32,
    /// Frame height in pixels (after decimation, if any).
    pub height: u32,
    /// RGBA8 bytes, row-major, length = `width * height * 4`.
    pub rgba: Vec<u8>,
}

/// Rescaled monochrome pixel values, ready for interactive windowing.
///
/// Produced by [`load_raw`]. The modality LUT (rescale slope/intercept)
/// has already been applied — for CT this means [`Self::values`] are in
/// Hounsfield Units. VOI windowing is *not* applied; that happens cheaply
/// on every drag delta via [`render_rgba`].
///
/// Images larger than 2048 px per side are decimated at load time;
/// [`Self::display_scale`] records the factor so measurements can convert
/// back to original-pixel units.
pub struct RawImage {
    /// Image width in pixels (after decimation).
    pub width: u32,
    /// Image height in pixels (after decimation).
    pub height: u32,
    /// Pixel values, row-major. Length = `width * height`. Already in
    /// modality units (HU on CT).
    pub values: Vec<f32>,
    /// `(center, width)` from `WindowCenter`/`WindowWidth` if present.
    pub default_window: Option<(f64, f64)>,
    /// Minimum finite value in [`Self::values`].
    pub min_val: f32,
    /// Maximum finite value in [`Self::values`].
    pub max_val: f32,
    /// `true` for `MONOCHROME1` — display inversion is needed.
    pub photometric_invert: bool,
    /// Decimation factor applied at load time. `1` = full resolution.
    /// Used to scale measurements back to original-pixel units.
    pub display_scale: u32,
}

/// Decode the pixel data of a DICOM file into a [`RawImage`].
///
/// Applies the modality LUT (rescale slope/intercept) but *not* the VOI
/// LUT. Decimates oversized images so that the longest side is at most
/// 2048 pixels (see `MAX_DISPLAY_DIM`).
///
/// # Errors
/// Returns an error when the file can't be opened, pixel data can't be
/// decoded, or the layout isn't supported (e.g. RGB / multi-sample inputs
/// the viewer doesn't yet handle).
pub fn load_raw(path: &Path) -> Result<RawImage> {
    let obj = open_file(path).with_context(|| format!("open {}", path.display()))?;
    let decoded = obj
        .decode_pixel_data()
        .with_context(|| format!("decode pixels {}", path.display()))?;

    let opts = ConvertOptions::new()
        .with_modality_lut(ModalityLutOption::Default)
        .with_voi_lut(VoiLutOption::Identity);
    let vals: Vec<f32> = decoded
        .to_vec_with_options(&opts)
        .with_context(|| format!("to_vec f32 {}", path.display()))?;

    let invert = matches!(
        decoded.photometric_interpretation(),
        PhotometricInterpretation::Monochrome1
    );

    let default_window = decoded
        .window()
        .ok()
        .flatten()
        .and_then(|ws| ws.first().map(|w| (w.center, w.width)));

    let mut raw = RawImage {
        width: decoded.columns(),
        height: decoded.rows(),
        values: vals,
        default_window,
        min_val: 0.0,
        max_val: 1.0,
        photometric_invert: invert,
        display_scale: 1,
    };

    // Downsample oversized images.
    let max_side = raw.width.max(raw.height);
    if max_side > MAX_DISPLAY_DIM {
        let scale = max_side.div_ceil(MAX_DISPLAY_DIM);
        raw.display_scale = scale;
        let new_w = (raw.width / scale).max(1);
        let new_h = (raw.height / scale).max(1);
        let mut new_vals = Vec::with_capacity((new_w * new_h) as usize);
        for y in 0..new_h {
            for x in 0..new_w {
                let sx = (x * scale) as usize;
                let sy = (y * scale) as usize;
                let idx = sy * (raw.width as usize) + sx;
                new_vals.push(raw.values[idx]);
            }
        }
        raw.width = new_w;
        raw.height = new_h;
        raw.values = new_vals;
    }

    let (mut mn, mut mx) = (f32::INFINITY, f32::NEG_INFINITY);
    for &v in &raw.values {
        if v.is_finite() {
            if v < mn {
                mn = v;
            }
            if v > mx {
                mx = v;
            }
        }
    }
    if !mn.is_finite() {
        mn = 0.0;
    }
    if !mx.is_finite() {
        mx = mn + 1.0;
    }
    raw.min_val = mn;
    raw.max_val = mx;

    // Sanity check — keeps the rest of the app from panicking on
    // mismatched buffer sizes (e.g. RGB or multi-sample inputs we don't
    // handle yet).
    let expected = (raw.width as usize)
        .checked_mul(raw.height as usize)
        .unwrap_or(0);
    if raw.width == 0 || raw.height == 0 || expected == 0 || raw.values.len() != expected {
        anyhow::bail!(
            "unsupported pixel layout: {}x{}, {} values",
            raw.width,
            raw.height,
            raw.values.len()
        );
    }
    Ok(raw)
}

/// Apply a window/level and convert a [`RawImage`] to RGBA8.
///
/// The output is grey-on-grey (R = G = B = level) with full alpha. Cheap
/// enough to call on every drag delta for interactive W/L. The effective
/// inversion is `raw.photometric_invert XOR user_invert`, so toggling
/// "invert" on a `MONOCHROME1` image cancels back to natural display.
pub fn render_rgba(raw: &RawImage, center: f64, width: f64, user_invert: bool) -> Vec<u8> {
    let w = width.max(1e-3);
    let lo = center - w / 2.0;
    let inv_w = 255.0 / w;
    let invert = raw.photometric_invert ^ user_invert;
    let n = raw.values.len();
    let mut out = vec![0u8; n * 4];
    for (i, &v) in raw.values.iter().enumerate() {
        let g = ((v as f64 - lo) * inv_w).round().clamp(0.0, 255.0);
        let mut gi = g as u8;
        if invert {
            gi = 255 - gi;
        }
        let o = i * 4;
        out[o] = gi;
        out[o + 1] = gi;
        out[o + 2] = gi;
        out[o + 3] = 255;
    }
    out
}

/// Pick a sensible default window for `raw`.
///
/// Returns the DICOM file's default window if present, otherwise a
/// centred full-range window over `[min_val, max_val]`.
pub fn auto_window(raw: &RawImage) -> (f64, f64) {
    raw.default_window.unwrap_or_else(|| {
        let c = ((raw.min_val + raw.max_val) as f64) / 2.0;
        let w = (raw.max_val - raw.min_val).max(1.0) as f64;
        (c, w)
    })
}

/// Convenience: load + render in one shot. Used by tests.
pub fn decode_to_rgba(path: &Path, window: WindowSetting) -> Result<DecodedFrame> {
    let raw = load_raw(path)?;
    let (center, width) = match window {
        WindowSetting::Auto => auto_window(&raw),
        WindowSetting::Manual { center, width } => (center, width),
    };
    let rgba = render_rgba(&raw, center, width, false);
    Ok(DecodedFrame {
        width: raw.width,
        height: raw.height,
        rgba,
    })
}

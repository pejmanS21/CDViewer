//! Series preview thumbnails for the sidebar.
//!
//! Decodes the first instance of a series at heavily-decimated resolution
//! and applies an auto window/level. Result is a small RGBA buffer ready
//! for `ColorImage::from_rgba_unmultiplied`.

use anyhow::Result;
use std::path::Path;

use crate::dcm::pixel::{auto_window, load_raw};

/// Small RGBA buffer ready for `ColorImage::from_rgba_unmultiplied`.
pub struct Thumbnail {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 bytes, row-major.
    pub rgba: Vec<u8>,
}

/// Render a square-ish thumbnail no larger than `max_dim` per side. We
/// reuse [`load_raw`] (which already caps at `MAX_DISPLAY_DIM`) and then
/// decimate further by nearest-neighbour. This avoids dragging in any
/// extra image-resize crate.
pub fn load_thumbnail(path: &Path, max_dim: u32) -> Result<Thumbnail> {
    let raw = load_raw(path)?;
    let max_side = raw.width.max(raw.height);
    let scale = max_side.div_ceil(max_dim).max(1);
    let w = (raw.width / scale).max(1);
    let h = (raw.height / scale).max(1);

    let (center, width) = auto_window(&raw);
    let w_f = width.max(1e-3);
    let lo = center - w_f / 2.0;
    let inv_w = 255.0 / w_f;
    let invert = raw.photometric_invert;

    let mut out = vec![0u8; (w * h * 4) as usize];
    let src_w = raw.width as usize;
    for y in 0..h {
        let sy = (y * scale) as usize;
        for x in 0..w {
            let sx = (x * scale) as usize;
            let v = raw.values[sy * src_w + sx] as f64;
            let g = ((v - lo) * inv_w).round().clamp(0.0, 255.0) as u8;
            let g = if invert { 255 - g } else { g };
            let o = ((y * w + x) * 4) as usize;
            out[o] = g;
            out[o + 1] = g;
            out[o + 2] = g;
            out[o + 3] = 255;
        }
    }

    Ok(Thumbnail {
        width: w,
        height: h,
        rgba: out,
    })
}

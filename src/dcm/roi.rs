//! ROI statistics + measurement units.

use crate::dcm::pixel::RawImage;
use crate::dcm::study::Instance;

/// First-moment statistics over a region of interest.
///
/// Produced by [`rect_stats`] and [`ellipse_stats`]. Values are in the
/// same units as the underlying [`RawImage::values`] — Hounsfield Units
/// on CT (because the modality LUT has been applied) and raw rescaled
/// pixel values otherwise.
#[derive(Debug, Clone, Copy)]
pub struct RoiStats {
    /// Arithmetic mean of pixels inside the ROI.
    pub mean: f64,
    /// Sample standard deviation (population formula with `count` in the
    /// denominator — adequate for a non-diagnostic overlay).
    pub std: f64,
    /// Minimum pixel value inside the ROI.
    pub min: f64,
    /// Maximum pixel value inside the ROI.
    pub max: f64,
    /// Number of pixels summed.
    pub count: usize,
}

impl RoiStats {
    /// Format the stats for overlay display, suffixing `HU` on CT.
    pub fn label(&self, modality: &str) -> String {
        let unit = if modality.eq_ignore_ascii_case("CT") {
            " HU"
        } else {
            ""
        };
        format!(
            "n={} μ={:.1}{u} σ={:.1} min={:.0} max={:.0}",
            self.count,
            self.mean,
            self.std,
            self.min,
            self.max,
            u = unit
        )
    }
}

fn bbox(p1: [f32; 2], p2: [f32; 2], w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = p1[0].min(p2[0]).max(0.0).round() as i32;
    let y0 = p1[1].min(p2[1]).max(0.0).round() as i32;
    let x1 = p1[0].max(p2[0]).round() as i32;
    let y1 = p1[1].max(p2[1]).round() as i32;
    let x0 = x0.clamp(0, w as i32 - 1) as u32;
    let y0 = y0.clamp(0, h as i32 - 1) as u32;
    let x1 = x1.clamp(0, w as i32 - 1) as u32;
    let y1 = y1.clamp(0, h as i32 - 1) as u32;
    (x0, y0, x1.max(x0), y1.max(y0))
}

/// Compute [`RoiStats`] over the axis-aligned rectangle spanned by `p1`
/// and `p2` (in displayed-image-pixel coordinates).
///
/// Returns `None` when the rectangle degenerates after clamping to the
/// image bounds.
pub fn rect_stats(raw: &RawImage, p1: [f32; 2], p2: [f32; 2]) -> Option<RoiStats> {
    let (x0, y0, x1, y1) = bbox(p1, p2, raw.width, raw.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut sum = 0.0f64;
    let mut sumsq = 0.0f64;
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    let mut count = 0usize;
    let stride = raw.width as usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let v = raw.values[y as usize * stride + x as usize] as f64;
            sum += v;
            sumsq += v * v;
            if v < mn {
                mn = v;
            }
            if v > mx {
                mx = v;
            }
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mean = sum / count as f64;
    let var = (sumsq / count as f64 - mean * mean).max(0.0);
    Some(RoiStats {
        mean,
        std: var.sqrt(),
        min: mn,
        max: mx,
        count,
    })
}

/// Compute [`RoiStats`] over the ellipse inscribed in the rectangle
/// spanned by `p1` and `p2` (in displayed-image-pixel coordinates).
///
/// Returns `None` when the bounding rectangle degenerates after clamping
/// to the image bounds.
pub fn ellipse_stats(raw: &RawImage, p1: [f32; 2], p2: [f32; 2]) -> Option<RoiStats> {
    let (x0, y0, x1, y1) = bbox(p1, p2, raw.width, raw.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let cx = (x0 as f64 + x1 as f64) / 2.0;
    let cy = (y0 as f64 + y1 as f64) / 2.0;
    let rx = ((x1 - x0) as f64 / 2.0).max(0.5);
    let ry = ((y1 - y0) as f64 / 2.0).max(0.5);
    let mut sum = 0.0f64;
    let mut sumsq = 0.0f64;
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    let mut count = 0usize;
    let stride = raw.width as usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x as f64 - cx) / rx;
            let dy = (y as f64 - cy) / ry;
            if dx * dx + dy * dy > 1.0 {
                continue;
            }
            let v = raw.values[y as usize * stride + x as usize] as f64;
            sum += v;
            sumsq += v * v;
            if v < mn {
                mn = v;
            }
            if v > mx {
                mx = v;
            }
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mean = sum / count as f64;
    let var = (sumsq / count as f64 - mean * mean).max(0.0);
    Some(RoiStats {
        mean,
        std: var.sqrt(),
        min: mn,
        max: mx,
        count,
    })
}

/// Distance between two displayed-pixel points in millimetres if pixel
/// spacing is known, else pixels.
pub fn length_label(p1: [f32; 2], p2: [f32; 2], inst: &Instance, raw: &RawImage) -> String {
    let dx_px = (p2[0] - p1[0]) as f64;
    let dy_px = (p2[1] - p1[1]) as f64;
    let scale = raw.display_scale.max(1) as f64;
    if let Some((row_sp, col_sp)) = inst.pixel_spacing {
        let dx = dx_px * col_sp * scale;
        let dy = dy_px * row_sp * scale;
        let mm = (dx * dx + dy * dy).sqrt();
        format!("{:.1} mm", mm)
    } else {
        let px = (dx_px * dx_px + dy_px * dy_px).sqrt();
        format!("{:.0} px", px)
    }
}

/// Angle at vertex `v` between rays to `p1` and `p2`, in degrees.
///
/// Returns `0.0` when either ray has near-zero length.
pub fn angle_deg(p1: [f32; 2], v: [f32; 2], p2: [f32; 2]) -> f64 {
    let a = (p1[0] - v[0], p1[1] - v[1]);
    let b = (p2[0] - v[0], p2[1] - v[1]);
    let dot = (a.0 * b.0 + a.1 * b.1) as f64;
    let la = ((a.0 * a.0 + a.1 * a.1) as f64).sqrt();
    let lb = ((b.0 * b.0 + b.1 * b.1) as f64).sqrt();
    if la < 1e-6 || lb < 1e-6 {
        return 0.0;
    }
    (dot / (la * lb)).clamp(-1.0, 1.0).acos().to_degrees()
}

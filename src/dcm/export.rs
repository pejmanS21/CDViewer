//! PNG export with optional annotation burn-in, plus a basic
//! anonymisation pass for DICOM files.

use crate::dcm::annotation::{Annotation, AnnotationStore};
use crate::dcm::pixel::{auto_window, load_raw, render_rgba, RawImage};
use crate::dcm::roi::{angle_deg, ellipse_stats, length_label, rect_stats};
use crate::dcm::study::{Instance, Series};
use anyhow::{Context, Result};
use dicom::core::value::{DataSetSequence, PrimitiveValue, Value};
use dicom::core::{DataElement, VR};
use dicom::object::open_file;
use dicom_dictionary_std::tags;
use std::fs;
use std::path::Path;

/// Plain RGBA buffer used as the export canvas.
struct Canvas {
    w: u32,
    h: u32,
    data: Vec<u8>,
}

impl Canvas {
    fn from_raw(raw: &RawImage, window: (f64, f64), invert: bool) -> Self {
        let data = render_rgba(raw, window.0, window.1, invert);
        Self {
            w: raw.width,
            h: raw.height,
            data,
        }
    }

    fn put(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as usize * self.w as usize + x as usize) * 4;
        self.data[i] = color[0];
        self.data[i + 1] = color[1];
        self.data[i + 2] = color[2];
        self.data[i + 3] = color[3];
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
        // Bresenham.
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            self.put(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn rect_outline(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
        self.line(x0, y0, x1, y0, color);
        self.line(x1, y0, x1, y1, color);
        self.line(x1, y1, x0, y1, color);
        self.line(x0, y1, x0, y0, color);
    }

    fn ellipse_outline(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: [u8; 4]) {
        // Sample 96 points around perimeter — good enough for export.
        let steps = 96;
        let mut prev: Option<(i32, i32)> = None;
        for i in 0..=steps {
            let t = (i as f32) * std::f32::consts::TAU / steps as f32;
            let x = (cx + rx * t.cos()).round() as i32;
            let y = (cy + ry * t.sin()).round() as i32;
            if let Some((px, py)) = prev {
                self.line(px, py, x, y, color);
            }
            prev = Some((x, y));
        }
    }

    fn write_png(&self, path: &Path) -> Result<()> {
        image::save_buffer(path, &self.data, self.w, self.h, image::ColorType::Rgba8)
            .with_context(|| format!("write png {}", path.display()))?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn export_current_view(
    inst: &Instance,
    raw: &RawImage,
    window: (f64, f64),
    invert: bool,
    annotations: &[Annotation],
    burn_annotations: bool,
    out_path: &Path,
    modality: &str,
) -> Result<()> {
    let mut canvas = Canvas::from_raw(raw, window, invert);
    if burn_annotations {
        burn_into_canvas(&mut canvas, annotations, raw, inst, modality);
    }
    canvas.write_png(out_path)
}

pub fn export_series_pngs(
    series: &Series,
    out_dir: &Path,
    store: &AnnotationStore,
    burn_annotations: bool,
) -> Result<usize> {
    fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    let mut written = 0usize;
    for (i, inst) in series.instances.iter().enumerate() {
        let raw = match load_raw(&inst.path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(path = %inst.path.display(), error = %e, "skip slice");
                continue;
            }
        };
        let window = auto_window(&raw);
        let anns = store.for_instance(&inst.sop_instance_uid);
        let mut canvas = Canvas::from_raw(&raw, window, false);
        if burn_annotations && !anns.is_empty() {
            burn_into_canvas(&mut canvas, anns, &raw, inst, &series.modality);
        }
        let name = format!("slice_{:04}.png", i + 1);
        canvas.write_png(&out_dir.join(&name))?;
        written += 1;
    }
    Ok(written)
}

fn burn_into_canvas(
    canvas: &mut Canvas,
    anns: &[Annotation],
    raw: &RawImage,
    inst: &Instance,
    modality: &str,
) {
    let yellow = [240, 220, 80, 255];
    let cyan = [120, 220, 255, 255];
    let green = [120, 240, 160, 255];
    for ann in anns {
        match ann {
            Annotation::Length { p1, p2 } => {
                canvas.line(
                    p1[0] as i32,
                    p1[1] as i32,
                    p2[0] as i32,
                    p2[1] as i32,
                    yellow,
                );
                let _ = length_label(*p1, *p2, inst, raw);
            }
            Annotation::Angle { p1, v, p2 } => {
                canvas.line(v[0] as i32, v[1] as i32, p1[0] as i32, p1[1] as i32, yellow);
                canvas.line(v[0] as i32, v[1] as i32, p2[0] as i32, p2[1] as i32, yellow);
                let _ = angle_deg(*p1, *v, *p2);
            }
            Annotation::Rect { p1, p2 } => {
                canvas.rect_outline(p1[0] as i32, p1[1] as i32, p2[0] as i32, p2[1] as i32, cyan);
                let _ = rect_stats(raw, *p1, *p2);
                let _ = modality;
            }
            Annotation::Ellipse { p1, p2 } => {
                let cx = (p1[0] + p2[0]) / 2.0;
                let cy = (p1[1] + p2[1]) / 2.0;
                let rx = (p2[0] - p1[0]).abs() / 2.0;
                let ry = (p2[1] - p1[1]).abs() / 2.0;
                canvas.ellipse_outline(cx, cy, rx, ry, green);
                let _ = ellipse_stats(raw, *p1, *p2);
            }
        }
    }
}

/// PS3.15 Basic Application Confidentiality — minimal subset. Strips
/// patient identifiers; keeps imaging tags.
pub fn anonymize_file(src: &Path, dst: &Path) -> Result<()> {
    let mut obj = open_file(src).with_context(|| format!("open {}", src.display()))?;

    let blank_pn = DataElement::new(
        tags::PATIENT_NAME,
        VR::PN,
        Value::Primitive(PrimitiveValue::from("Anonymized")),
    );
    let blank_id = DataElement::new(
        tags::PATIENT_ID,
        VR::LO,
        Value::Primitive(PrimitiveValue::from("ANON")),
    );
    let blank_date = DataElement::new(
        tags::PATIENT_BIRTH_DATE,
        VR::DA,
        Value::Primitive(PrimitiveValue::from("")),
    );
    let blank_sex = DataElement::new(
        tags::PATIENT_SEX,
        VR::CS,
        Value::Primitive(PrimitiveValue::from("")),
    );
    let blank_acc = DataElement::new(
        tags::ACCESSION_NUMBER,
        VR::SH,
        Value::Primitive(PrimitiveValue::from("")),
    );
    let blank_ref = DataElement::new(
        tags::REFERRING_PHYSICIAN_NAME,
        VR::PN,
        Value::Primitive(PrimitiveValue::from("")),
    );
    let blank_op = DataElement::new(
        tags::OPERATORS_NAME,
        VR::PN,
        Value::Primitive(PrimitiveValue::from("")),
    );

    for e in [
        blank_pn, blank_id, blank_date, blank_sex, blank_acc, blank_ref, blank_op,
    ] {
        obj.put_element(e);
    }

    // Drop tags entirely where present.
    for tag in [
        tags::INSTITUTION_NAME,
        tags::INSTITUTION_ADDRESS,
        tags::STATION_NAME,
        tags::PATIENT_AGE,
        tags::PATIENT_ADDRESS,
        tags::PATIENT_TELEPHONE_NUMBERS,
    ] {
        let _ = obj.remove_element(tag);
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).ok();
    }
    obj.write_to_file(dst)
        .with_context(|| format!("write {}", dst.display()))?;
    Ok(())
}

pub fn anonymize_study(study: &crate::dcm::Study, out_dir: &Path) -> Result<(usize, usize)> {
    let mut ok = 0usize;
    let mut fail = 0usize;
    fs::create_dir_all(out_dir).ok();
    for series in &study.series {
        let series_dir = out_dir.join(safe_name(&series.series_instance_uid));
        for (i, inst) in series.instances.iter().enumerate() {
            let out = series_dir.join(format!("{:04}.dcm", i + 1));
            match anonymize_file(&inst.path, &out) {
                Ok(()) => ok += 1,
                Err(e) => {
                    tracing::warn!(error = %e, path = %inst.path.display(), "anonymize failed");
                    fail += 1;
                }
            }
        }
    }
    Ok((ok, fail))
}

// Squash dot-separated UIDs into something filesystem-safe.
fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .chars()
        .take(40)
        .collect()
}

#[allow(dead_code)]
fn _force_use(
    d: DataSetSequence<dicom::object::InMemDicomObject>,
) -> DataSetSequence<dicom::object::InMemDicomObject> {
    d
}

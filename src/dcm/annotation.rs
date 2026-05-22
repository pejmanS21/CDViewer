//! User annotations + measurements, persisted to a sidecar JSON.
//!
//! Coordinates are stored in **displayed image pixels** (i.e. post the
//! decimation that `load_raw` applies). Conversion to real-world
//! millimetres uses `Instance::pixel_spacing` scaled by
//! `RawImage::display_scale`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// A single user annotation on a DICOM instance.
///
/// All point coordinates are in **displayed image pixels** — i.e. they
/// index into [`RawImage::values`](crate::dcm::pixel::RawImage::values)
/// directly. To convert to millimetres, multiply by
/// `Instance::pixel_spacing × RawImage::display_scale` (see
/// [`crate::dcm::roi::length_label`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Annotation {
    /// Two-point distance measurement.
    Length {
        /// First endpoint.
        p1: [f32; 2],
        /// Second endpoint.
        p2: [f32; 2],
    },
    /// Three-point angle: rays from `v` to `p1` and `p2`.
    Angle {
        /// One ray endpoint.
        p1: [f32; 2],
        /// Vertex of the angle.
        v: [f32; 2],
        /// Other ray endpoint.
        p2: [f32; 2],
    },
    /// Axis-aligned rectangular ROI.
    Rect {
        /// One corner.
        p1: [f32; 2],
        /// Opposite corner.
        p2: [f32; 2],
    },
    /// Ellipse inscribed in the rectangle spanned by `p1` and `p2`.
    Ellipse {
        /// One corner of the bounding rectangle.
        p1: [f32; 2],
        /// Opposite corner of the bounding rectangle.
        p2: [f32; 2],
    },
}

impl Annotation {
    /// Short human-readable label for the annotation kind.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Length { .. } => "Length",
            Self::Angle { .. } => "Angle",
            Self::Rect { .. } => "Rect ROI",
            Self::Ellipse { .. } => "Ellipse ROI",
        }
    }
}

/// All annotations on disk, keyed by SOP Instance UID.
///
/// Saved as pretty JSON to `<data_dir>/annotations.json` on every push,
/// clear, or undo (the viewer favours safety over write-batching here —
/// these files are tiny).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationStore {
    /// SOP Instance UID → annotation list (in insertion order).
    pub by_instance: BTreeMap<String, Vec<Annotation>>,
}

impl AnnotationStore {
    /// Load from a JSON sidecar. Missing or malformed files yield
    /// [`Self::default`] — annotations are an optional layer, not a hard
    /// requirement.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
    /// Atomically(-ish) write the store to `path` as pretty JSON, creating
    /// the parent directory if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).ok();
        }
        let text = serde_json::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }
    /// Borrow the annotation list for `uid`, or an empty slice if absent.
    pub fn for_instance(&self, uid: &str) -> &[Annotation] {
        self.by_instance
            .get(uid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
    /// Append `ann` to the list for `uid`.
    pub fn push(&mut self, uid: &str, ann: Annotation) {
        self.by_instance
            .entry(uid.to_string())
            .or_default()
            .push(ann);
    }
    /// Drop every annotation on `uid`.
    pub fn clear_instance(&mut self, uid: &str) {
        self.by_instance.remove(uid);
    }
    /// Pop the most recently pushed annotation on `uid`. Removes the
    /// per-instance entry entirely if the list becomes empty.
    pub fn pop_last(&mut self, uid: &str) {
        if let Some(v) = self.by_instance.get_mut(uid) {
            v.pop();
            if v.is_empty() {
                self.by_instance.remove(uid);
            }
        }
    }
}

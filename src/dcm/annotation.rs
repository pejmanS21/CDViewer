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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Annotation {
    Length {
        p1: [f32; 2],
        p2: [f32; 2],
    },
    Angle {
        p1: [f32; 2],
        v: [f32; 2],
        p2: [f32; 2],
    },
    Rect {
        p1: [f32; 2],
        p2: [f32; 2],
    },
    Ellipse {
        p1: [f32; 2],
        p2: [f32; 2],
    },
}

impl Annotation {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Length { .. } => "Length",
            Self::Angle { .. } => "Angle",
            Self::Rect { .. } => "Rect ROI",
            Self::Ellipse { .. } => "Ellipse ROI",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationStore {
    pub by_instance: BTreeMap<String, Vec<Annotation>>,
}

impl AnnotationStore {
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).ok();
        }
        let text = serde_json::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }
    pub fn for_instance(&self, uid: &str) -> &[Annotation] {
        self.by_instance
            .get(uid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
    pub fn push(&mut self, uid: &str, ann: Annotation) {
        self.by_instance
            .entry(uid.to_string())
            .or_default()
            .push(ann);
    }
    pub fn clear_instance(&mut self, uid: &str) {
        self.by_instance.remove(uid);
    }
    pub fn pop_last(&mut self, uid: &str) {
        if let Some(v) = self.by_instance.get_mut(uid) {
            v.pop();
            if v.is_empty() {
                self.by_instance.remove(uid);
            }
        }
    }
}

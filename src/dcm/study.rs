//! In-memory model of loaded DICOM studies.
//!
//! Only metadata is held here; pixel data is decoded on demand by
//! [`crate::dcm::pixel`] and cached as egui textures in the app layer.

use std::path::PathBuf;

#[allow(dead_code)] // some fields surface in the metadata panel (Milestone 6)
#[derive(Debug, Clone)]
pub struct Instance {
    pub path: PathBuf,
    pub sop_instance_uid: String,
    pub instance_number: i32,
    pub rows: u16,
    pub cols: u16,
    pub modality: String,
    pub photometric: String,
    pub window_center: Option<f64>,
    pub window_width: Option<f64>,
    pub rescale_slope: f64,
    pub rescale_intercept: f64,
    pub pixel_spacing: Option<(f64, f64)>,
    /// MG-specific: "CC", "MLO", etc.
    pub view_position: Option<String>,
    /// MG-specific: "L" or "R".
    pub image_laterality: Option<String>,
    /// ImagePositionPatient z — gold-standard slice ordering for CT/MR.
    /// `None` when the series doesn't expose it (e.g. MG, US).
    pub image_position_z: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Series {
    pub series_instance_uid: String,
    pub series_number: i32,
    pub modality: String,
    pub description: String,
    pub instances: Vec<Instance>,
}

impl Series {
    pub fn is_mammography(&self) -> bool {
        self.modality.eq_ignore_ascii_case("MG")
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Study {
    pub study_instance_uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub study_date: String,
    pub study_description: String,
    pub modalities: Vec<String>,
    pub series: Vec<Series>,
}

impl Study {
    pub fn label(&self) -> String {
        let mods = if self.modalities.is_empty() {
            "—".into()
        } else {
            self.modalities.join("/")
        };
        let name = if self.patient_name.is_empty() {
            "(no name)".into()
        } else {
            self.patient_name.clone()
        };
        format!("{name} [{mods}] {}", self.study_date)
    }
}

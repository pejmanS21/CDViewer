//! In-memory model of loaded DICOM studies.
//!
//! Only metadata is held here; pixel data is decoded on demand by
//! [`crate::dcm::pixel`] and cached as egui textures in the app layer.

use std::path::PathBuf;

/// A single DICOM image (one frame, one SOP Instance).
///
/// Metadata only — pixel data is decoded on demand by
/// [`crate::dcm::pixel::load_raw`]. The whole struct is intentionally
/// cheap to clone so it can be passed across rendering / measurement /
/// export paths.
#[allow(dead_code)] // some fields surface in the metadata panel (Milestone 6)
#[derive(Debug, Clone)]
pub struct Instance {
    /// On-disk path to the DICOM file.
    pub path: PathBuf,
    /// SOP Instance UID — used as the cache key for pixel data,
    /// annotations, and rendered textures.
    pub sop_instance_uid: String,
    /// `InstanceNumber` — slice ordering fallback when ImagePositionPatient
    /// is missing.
    pub instance_number: i32,
    /// Pixel `Rows`.
    pub rows: u16,
    /// Pixel `Columns`.
    pub cols: u16,
    /// `Modality` (CT, MR, MG, CR, …).
    pub modality: String,
    /// `PhotometricInterpretation`. `MONOCHROME1` requires display
    /// inversion.
    pub photometric: String,
    /// Default VOI window centre, if present in the file.
    pub window_center: Option<f64>,
    /// Default VOI window width, if present in the file.
    pub window_width: Option<f64>,
    /// `RescaleSlope` (default 1.0).
    pub rescale_slope: f64,
    /// `RescaleIntercept` (default 0.0).
    pub rescale_intercept: f64,
    /// `(row_spacing_mm, col_spacing_mm)` from `PixelSpacing`, when
    /// present. Drives millimetre measurements.
    pub pixel_spacing: Option<(f64, f64)>,
    /// MG-specific: "CC", "MLO", etc.
    pub view_position: Option<String>,
    /// MG-specific: "L" or "R".
    pub image_laterality: Option<String>,
    /// ImagePositionPatient z — gold-standard slice ordering for CT/MR.
    /// `None` when the series doesn't expose it (e.g. MG, US).
    pub image_position_z: Option<f64>,
}

/// One DICOM series (typically one acquisition, many instances).
///
/// Instances are pre-sorted by `ImagePositionPatient` z when available
/// (CT/MR), otherwise by `InstanceNumber`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Series {
    /// Series Instance UID — stable across re-scans of the same disc.
    pub series_instance_uid: String,
    /// `SeriesNumber` — drives sidebar ordering within a study.
    pub series_number: i32,
    /// Series modality. Empty when the source files don't agree.
    pub modality: String,
    /// `SeriesDescription`, if present.
    pub description: String,
    /// Slices ordered for radiologist-friendly scrolling.
    pub instances: Vec<Instance>,
}

impl Series {
    /// `true` when the series modality is mammography (MG). Drives the
    /// 2×2 hanging-protocol path in [`crate::app::DicomViewerApp::select_series`].
    pub fn is_mammography(&self) -> bool {
        self.modality.eq_ignore_ascii_case("MG")
    }
}

/// A DICOM study — one patient encounter, possibly multiple series.
///
/// Studies are sorted newest-first by [`Self::study_date`] in
/// [`crate::dcm::loader::load_folder`].
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Study {
    /// Study Instance UID.
    pub study_instance_uid: String,
    /// `PatientName`, raw DICOM-encoded string.
    pub patient_name: String,
    /// `PatientID`.
    pub patient_id: String,
    /// `StudyDate` (YYYYMMDD). Empty when missing.
    pub study_date: String,
    /// `StudyDescription`.
    pub study_description: String,
    /// Distinct modality codes seen across the study's series.
    pub modalities: Vec<String>,
    /// Series, ordered by `SeriesNumber`.
    pub series: Vec<Series>,
}

impl Study {
    /// One-line label used in the sidebar header (e.g.
    /// `"Doe^John [CT/MG] 20251104"`).
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

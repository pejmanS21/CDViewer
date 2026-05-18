//! Top-level egui application: holds shared state and drives the UI.

use crate::config::{Config, Paths};
use crate::dcm::annotation::{Annotation, AnnotationStore};
use crate::dcm::{self, metadata::TagRow, RawImage, Study};
use crate::ui::{self, GridLayout};
use eframe::CreationContext;
use egui::{Context, TextureHandle, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct DicomViewerApp {
    pub paths: Paths,
    pub config: Config,
    pub ui_state: ui::UiState,

    pub studies: Vec<Study>,
    pub raw_cache: HashMap<String, Arc<RawImage>>,
    pub raw_failures: HashMap<String, String>,
    pub metadata_cache: HashMap<String, Arc<Vec<TagRow>>>,
    pub metadata_filter: String,

    pub grid: GridLayout,
    pub cells: Vec<CellState>,
    pub active_cell: usize,

    pub annotation_store: AnnotationStore,
    pub annotations_path: PathBuf,

    pub last_error: Option<String>,
    pub last_info: Option<String>,
}

/// All viewport state belongs to a cell, so transforms persist when
/// switching layouts and so dragging a different series onto a cell
/// resets only that cell.
#[derive(Clone)]
pub struct CellState {
    pub series_ref: Option<(usize, usize)>,
    pub slice: usize,
    pub pan: Vec2,
    pub zoom: f32,
    pub rotation_quarter: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    pub invert: bool,
    pub window: Option<(f64, f64)>,
    pub error: Option<String>,
    // Cached uploaded texture, keyed by (uid, window, invert) so we know
    // when to regenerate.
    pub tex: Option<TextureHandle>,
    pub tex_uid: Option<String>,
    pub tex_window: Option<(f64, f64)>,
    pub tex_invert: bool,
}

impl Default for CellState {
    fn default() -> Self {
        Self {
            series_ref: None,
            slice: 0,
            pan: Vec2::ZERO,
            zoom: 1.0,
            rotation_quarter: 0,
            flip_h: false,
            flip_v: false,
            invert: false,
            window: None,
            error: None,
            tex: None,
            tex_uid: None,
            tex_window: None,
            tex_invert: false,
        }
    }
}

impl CellState {
    pub fn reset_view(&mut self) {
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.rotation_quarter = 0;
        self.flip_h = false;
        self.flip_v = false;
        self.invert = false;
        self.window = None;
        // Texture stays cached unless invert/window changed; the renderer
        // notices the mismatch and regenerates.
    }

    pub fn assign(&mut self, series_ref: (usize, usize), slice: usize) {
        *self = CellState::default();
        self.series_ref = Some(series_ref);
        self.slice = slice;
    }
}

impl DicomViewerApp {
    pub fn new(cc: &CreationContext<'_>, paths: Paths, config: Config) -> Self {
        apply_visuals(&cc.egui_ctx, config.ui.dark_mode);
        let needs_disclaimer = !config.disclaimer_acknowledged;
        let ui_state = ui::UiState::new(needs_disclaimer, &config);
        let annotations_path = paths.data_dir.join("annotations.json");
        let annotation_store = AnnotationStore::load(&annotations_path);
        Self {
            paths,
            config,
            ui_state,
            studies: Vec::new(),
            raw_cache: HashMap::new(),
            raw_failures: HashMap::new(),
            metadata_cache: HashMap::new(),
            metadata_filter: String::new(),
            grid: GridLayout::OneByOne,
            cells: vec![CellState::default()],
            active_cell: 0,
            annotation_store,
            annotations_path,
            last_error: None,
            last_info: None,
        }
    }

    pub fn save_annotations(&self) {
        if let Err(e) = self.annotation_store.save(&self.annotations_path) {
            tracing::warn!(error = %e, "annotation save failed");
        }
    }

    pub fn push_annotation(&mut self, sop_uid: &str, ann: Annotation) {
        self.annotation_store.push(sop_uid, ann);
        self.save_annotations();
    }

    pub fn clear_annotations_for_active(&mut self) {
        if let Some(inst) = self.active_instance().cloned() {
            self.annotation_store.clear_instance(&inst.sop_instance_uid);
            self.save_annotations();
        }
    }

    pub fn undo_last_annotation_for_active(&mut self) {
        if let Some(inst) = self.active_instance().cloned() {
            self.annotation_store.pop_last(&inst.sop_instance_uid);
            self.save_annotations();
        }
    }

    pub fn set_grid(&mut self, g: GridLayout) {
        let n = g.cell_count();
        self.grid = g;
        while self.cells.len() < n {
            self.cells.push(CellState::default());
        }
        self.cells.truncate(n);
        if self.active_cell >= n {
            self.active_cell = 0;
        }
    }

    pub fn open_folder(&mut self, folder: &Path) {
        self.last_error = None;
        match dcm::loader::load_folder(folder) {
            Ok(studies) if studies.is_empty() => {
                self.last_error = Some(format!("No DICOM files found in {}", folder.display()));
                tracing::warn!(path = %folder.display(), "no DICOM files");
            }
            Ok(studies) => {
                self.studies = studies;
                self.raw_cache.clear();
                self.raw_failures.clear();
                self.metadata_cache.clear();
                for c in &mut self.cells {
                    *c = CellState::default();
                }
                // Auto-select first study/series for first cell.
                if !self.studies.is_empty() && !self.studies[0].series.is_empty() {
                    self.select_series(0, 0);
                }
                tracing::info!(studies = self.studies.len(), "loaded folder");
            }
            Err(e) => {
                tracing::error!(error = %e, "load_folder failed");
                self.last_error = Some(format!("{e:#}"));
            }
        }
    }

    pub fn open_file(&mut self, file: &Path) {
        if let Some(parent) = file.parent() {
            self.open_folder(parent);
        }
    }

    /// Click on a series in the sidebar. MG with ≥4 views auto-fills 2×2;
    /// everything else goes to the active cell.
    pub fn select_series(&mut self, study_idx: usize, series_idx: usize) {
        let (is_mg, order) = match self
            .studies
            .get(study_idx)
            .and_then(|s| s.series.get(series_idx))
        {
            Some(series) => (
                series.is_mammography() && series.instances.len() >= 4,
                mg_slice_order(series),
            ),
            None => return,
        };
        if is_mg {
            self.set_grid(GridLayout::TwoByTwo);
            for (cell_idx, slice_idx) in order.into_iter().enumerate().take(4) {
                self.cells[cell_idx].assign((study_idx, series_idx), slice_idx);
            }
            self.active_cell = 0;
        } else {
            let cell = self.active_cell.min(self.cells.len().saturating_sub(1));
            self.cells[cell].assign((study_idx, series_idx), 0);
        }
    }

    /// Drop a series onto a specific cell (drag-and-drop target).
    pub fn drop_series_onto_cell(&mut self, cell_idx: usize, series_ref: (usize, usize)) {
        if cell_idx >= self.cells.len() {
            tracing::warn!(cell_idx, len = self.cells.len(), "drop: cell out of range");
            return;
        }
        let (si, se) = series_ref;
        let valid = self
            .studies
            .get(si)
            .and_then(|s| s.series.get(se))
            .is_some_and(|s| !s.instances.is_empty());
        if !valid {
            tracing::warn!(si, se, "drop: invalid or empty series");
            return;
        }
        tracing::info!(cell_idx, si, se, "drop series onto cell");
        self.cells[cell_idx].assign(series_ref, 0);
        self.active_cell = cell_idx;
    }

    pub fn raw_for(&mut self, sop_uid: &str, path: &Path) -> Option<Arc<RawImage>> {
        if let Some(r) = self.raw_cache.get(sop_uid) {
            return Some(r.clone());
        }
        if self.raw_failures.contains_key(sop_uid) {
            return None;
        }
        match dcm::load_raw(path) {
            Ok(r) => {
                let arc = Arc::new(r);
                self.raw_cache.insert(sop_uid.to_string(), arc.clone());
                Some(arc)
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "raw load failed");
                self.raw_failures.insert(sop_uid.to_string(), format!("{e:#}"));
                None
            }
        }
    }

    pub fn handle_dropped_files(&mut self, ctx: &Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        for f in dropped {
            if let Some(path) = f.path {
                if path.is_dir() {
                    self.open_folder(&path);
                } else {
                    self.open_file(&path);
                }
                break;
            }
        }
    }

    pub fn reset_active_cell(&mut self) {
        if let Some(c) = self.cells.get_mut(self.active_cell) {
            c.reset_view();
        }
    }

    /// Return the instance the active cell is currently showing, if any.
    pub fn active_instance(&self) -> Option<&crate::dcm::Instance> {
        let cell = self.cells.get(self.active_cell)?;
        let (si, se) = cell.series_ref?;
        let series = self.studies.get(si)?.series.get(se)?;
        series.instances.get(cell.slice)
    }

    pub fn metadata_for(&mut self, sop_uid: &str, path: &Path) -> Option<Arc<Vec<TagRow>>> {
        if let Some(rows) = self.metadata_cache.get(sop_uid) {
            return Some(rows.clone());
        }
        match dcm::metadata::collect_tags(path) {
            Ok(rows) => {
                let arc = Arc::new(rows);
                self.metadata_cache.insert(sop_uid.to_string(), arc.clone());
                Some(arc)
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "metadata load failed");
                None
            }
        }
    }
}

/// Standard mammography panel order: RCC, LCC, RMLO, LMLO.
fn mg_slice_order(series: &crate::dcm::Series) -> Vec<usize> {
    let key = |inst: &crate::dcm::Instance| -> u8 {
        let lat = inst.image_laterality.as_deref().unwrap_or("");
        let view = inst.view_position.as_deref().unwrap_or("");
        match (lat, view) {
            ("R", "CC") => 0,
            ("L", "CC") => 1,
            ("R", "MLO") => 2,
            ("L", "MLO") => 3,
            _ => 4,
        }
    };
    let mut indexed: Vec<(u8, usize)> = series
        .instances
        .iter()
        .enumerate()
        .map(|(i, inst)| (key(inst), i))
        .collect();
    indexed.sort_by_key(|(k, _)| *k);
    indexed.into_iter().map(|(_, i)| i).collect()
}

fn apply_visuals(ctx: &Context, dark: bool) {
    if dark {
        ctx.set_visuals(egui::Visuals::dark());
    } else {
        ctx.set_visuals(egui::Visuals::light());
    }
}

impl eframe::App for DicomViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ctx);
        ui::draw(ctx, self);
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.config.save(&self.paths) {
            tracing::warn!(error = %e, "failed to save config on exit");
        }
    }
}

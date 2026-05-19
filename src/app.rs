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
    /// Sidebar previews. Lazily populated one-per-frame so the UI never
    /// stalls behind a chain of pixel decodes.
    pub thumbnails: HashMap<String, ThumbnailState>,

    pub grid: GridLayout,
    pub cells: Vec<CellState>,
    pub active_cell: usize,

    pub annotation_store: AnnotationStore,
    pub annotations_path: PathBuf,

    pub last_error: Option<String>,
    pub last_info: Option<String>,

    /// Drag-drop drops are deferred until the *next* frame. Applying them
    /// inline during `viewport::draw` would free the cell's TextureHandle
    /// while shapes referencing that texture are still in the current
    /// frame's command buffer — wgpu panics with
    /// "Texture … has been destroyed".
    pending_drops: Vec<(usize, (usize, usize))>,

    /// Folder to load on the first update tick. Set when the binary is
    /// launched with a CLI arg, with `DICOM_VIEWER_DATA`, or from a CD
    /// where a sibling `DICOM/` directory exists. Deferred so the window
    /// paints once before the (potentially slow) folder scan runs.
    pending_startup: Option<PathBuf>,
}

/// How the image is sized relative to its cell rect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FitMode {
    /// Default: fit the image inside the cell, preserving aspect.
    #[default]
    Contain,
    /// Mammography hanging protocol: fill the cell vertically, let the
    /// other axis overflow / underflow as needed.
    Height,
}

/// Horizontal anchor inside the cell rect when the image doesn't fill the
/// cell width. MG cells anchor the chest-wall side to the inner edge of
/// the 2×2 (Right for right breasts, Left for left breasts).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HAnchor {
    #[default]
    Center,
    Left,
    Right,
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
    /// Fit mode + horizontal anchor are set by `apply_mg_hanging` and
    /// stay at their defaults for every other modality.
    pub fit_mode: FitMode,
    pub h_anchor: HAnchor,
    pub error: Option<String>,
    // Cached uploaded texture, keyed by (uid, window, invert) so we know
    // when to regenerate.
    pub tex: Option<TextureHandle>,
    pub tex_uid: Option<String>,
    pub tex_window: Option<(f64, f64)>,
    pub tex_invert: bool,
}

/// Which breast the cell holds — drives the hanging protocol.
#[derive(Debug, Clone, Copy)]
pub enum MgSide {
    Right,
    Left,
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
            fit_mode: FitMode::default(),
            h_anchor: HAnchor::default(),
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
        self.fit_mode = FitMode::default();
        self.h_anchor = HAnchor::default();
        // Texture stays cached unless invert/window changed; the renderer
        // notices the mismatch and regenerates.
    }

    pub fn assign(&mut self, series_ref: (usize, usize), slice: usize) {
        *self = CellState::default();
        self.series_ref = Some(series_ref);
        self.slice = slice;
    }

    /// Configure this cell for the MG hanging protocol: fit-to-height,
    /// chest-wall anchored to the inner edge of the 2×2, left breasts
    /// mirrored so chest walls face each other.
    pub fn apply_mg_hanging(&mut self, side: MgSide) {
        self.fit_mode = FitMode::Height;
        match side {
            MgSide::Right => {
                self.flip_h = false;
                self.h_anchor = HAnchor::Right;
            }
            MgSide::Left => {
                self.flip_h = true;
                self.h_anchor = HAnchor::Left;
            }
        }
    }
}

impl DicomViewerApp {
    pub fn new(
        cc: &CreationContext<'_>,
        paths: Paths,
        config: Config,
        startup_folder: Option<PathBuf>,
    ) -> Self {
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
            thumbnails: HashMap::new(),
            grid: GridLayout::OneByOne,
            cells: vec![CellState::default()],
            active_cell: 0,
            annotation_store,
            annotations_path,
            last_error: None,
            last_info: None,
            pending_drops: Vec::new(),
            pending_startup: startup_folder,
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
                self.thumbnails.clear();
                for c in &mut self.cells {
                    *c = CellState::default();
                }
                // Auto-arrange: prefer MG hanging protocol across the
                // whole study (handles 4-series-of-1-instance layouts too),
                // otherwise drop the first series into the first cell.
                if !self.studies.is_empty()
                    && !self.try_arrange_mg_study(0)
                    && !self.studies[0].series.is_empty()
                {
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

    /// Click on a series in the sidebar. First tries the MG hanging
    /// protocol across the whole study (covers the 4-series-of-1-instance
    /// case). If that doesn't apply, falls back to single-series MG (≥4
    /// instances in this series) or assigning to the active cell.
    pub fn select_series(&mut self, study_idx: usize, series_idx: usize) {
        // Whole-study MG: works regardless of whether the four views are
        // packed in one series or split across four.
        if self.is_mg_study(study_idx) && self.try_arrange_mg_study(study_idx) {
            return;
        }

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
            // Apply the hanging protocol so the chest walls face inward.
            if let Some(series) = self.studies[study_idx].series.get(series_idx) {
                for cell_idx in 0..4 {
                    let inst_idx = self.cells[cell_idx].slice;
                    if let Some(inst) = series.instances.get(inst_idx) {
                        if let Some(side) = mg_side(inst) {
                            self.cells[cell_idx].apply_mg_hanging(side);
                        }
                    }
                }
            }
            self.active_cell = 0;
        } else {
            let cell = self.active_cell.min(self.cells.len().saturating_sub(1));
            self.cells[cell].assign((study_idx, series_idx), 0);
        }
    }

    /// Does this study contain any MG-modality series?
    fn is_mg_study(&self, study_idx: usize) -> bool {
        self.studies
            .get(study_idx)
            .map(|s| s.series.iter().any(|se| se.is_mammography()))
            .unwrap_or(false)
    }

    /// Look across every MG series in the study and assemble a 2×2
    /// butterfly layout when there's at least one right-breast and one
    /// left-breast view (the minimum needed to be useful).
    ///
    /// Three placement modes, in priority order:
    /// 1. Strict — RCC, LCC, RMLO, LMLO all present: place in that order.
    /// 2. View-known — at least 2 R and 2 L with `ViewPosition` set: sort
    ///    each side's queue by view (CC before MLO) so the top row is the
    ///    cranio-caudal pair.
    /// 3. Laterality-only — at least 2 R and 2 L but `ViewPosition` is
    ///    missing (common in some PACS exports): pair them in
    ///    series/instance order.
    ///
    /// Every populated cell gets the hanging protocol applied (fit to
    /// height, anchor chest wall to the inner edge of the 2×2, mirror
    /// left breasts).
    pub fn try_arrange_mg_study(&mut self, study_idx: usize) -> bool {
        let Some(study) = self.studies.get(study_idx) else {
            return false;
        };

        // (view_priority, series_idx, instance_idx). view_priority sorts
        // CC < MLO < unknown so CCs end up on the top row when known.
        let mut rights: Vec<(u8, usize, usize)> = Vec::new();
        let mut lefts: Vec<(u8, usize, usize)> = Vec::new();
        for (si, series) in study.series.iter().enumerate() {
            if !series.is_mammography() {
                continue;
            }
            for (ii, inst) in series.instances.iter().enumerate() {
                let Some(side) = mg_side(inst) else {
                    continue;
                };
                let vp = match inst.view_position.as_deref() {
                    Some("CC") => 0u8,
                    Some("MLO") => 1u8,
                    _ => 2u8,
                };
                match side {
                    MgSide::Right => rights.push((vp, si, ii)),
                    MgSide::Left => lefts.push((vp, si, ii)),
                }
            }
        }
        if rights.len() < 2 || lefts.len() < 2 {
            return false;
        }
        rights.sort_by_key(|&(vp, si, ii)| (vp, si, ii));
        lefts.sort_by_key(|&(vp, si, ii)| (vp, si, ii));

        self.set_grid(GridLayout::TwoByTwo);
        let pick =
            |v: &Vec<(u8, usize, usize)>, idx: usize| -> (usize, usize) { (v[idx].1, v[idx].2) };
        let (r0_si, r0_ii) = pick(&rights, 0);
        let (l0_si, l0_ii) = pick(&lefts, 0);
        let (r1_si, r1_ii) = pick(&rights, 1);
        let (l1_si, l1_ii) = pick(&lefts, 1);

        self.cells[0].assign((study_idx, r0_si), r0_ii);
        self.cells[0].apply_mg_hanging(MgSide::Right);
        self.cells[1].assign((study_idx, l0_si), l0_ii);
        self.cells[1].apply_mg_hanging(MgSide::Left);
        self.cells[2].assign((study_idx, r1_si), r1_ii);
        self.cells[2].apply_mg_hanging(MgSide::Right);
        self.cells[3].assign((study_idx, l1_si), l1_ii);
        self.cells[3].apply_mg_hanging(MgSide::Left);
        self.active_cell = 0;
        tracing::info!(
            study_idx,
            r = rights.len(),
            l = lefts.len(),
            "applied MG hanging protocol (study-level)"
        );
        true
    }

    /// Queue a drag-drop assignment. Validated now (so we don't push
    /// garbage), applied next frame (so the cell's current TextureHandle
    /// survives the rest of this frame's GPU submission).
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
        tracing::info!(cell_idx, si, se, "queue drop series onto cell");
        // Replace any earlier-queued drop for the same cell — last one wins.
        self.pending_drops.retain(|(c, _)| *c != cell_idx);
        self.pending_drops.push((cell_idx, series_ref));
    }

    /// Apply queued drops. Called at the *start* of each frame, before
    /// any rendering, so dropping a CellState's old TextureHandle is safe.
    fn flush_pending_drops(&mut self) {
        if self.pending_drops.is_empty() {
            return;
        }
        let drops = std::mem::take(&mut self.pending_drops);
        for (cell_idx, series_ref) in drops {
            if cell_idx >= self.cells.len() {
                continue;
            }
            self.cells[cell_idx].assign(series_ref, 0);
            self.active_cell = cell_idx;
        }
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
                self.raw_failures
                    .insert(sop_uid.to_string(), format!("{e:#}"));
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

    /// Sidebar asks per row: "do you have a thumbnail for this series?"
    /// We register a `Pending` entry on first miss; the per-frame pump in
    /// [`pump_thumbnails`] decodes one at a time.
    pub fn thumbnail_for(&mut self, series_uid: &str) -> Option<TextureHandle> {
        match self.thumbnails.get(series_uid) {
            Some(ThumbnailState::Ready(t)) => Some(t.clone()),
            Some(_) => None,
            None => {
                self.thumbnails
                    .insert(series_uid.to_string(), ThumbnailState::Pending);
                None
            }
        }
    }

    /// Decode at most one pending sidebar thumbnail per frame, so the UI
    /// thread isn't blocked by a long chain of MG-sized decodes.
    pub fn pump_thumbnails(&mut self, ctx: &Context) {
        let next: Option<(String, PathBuf)> = self
            .thumbnails
            .iter()
            .find_map(|(uid, state)| matches!(state, ThumbnailState::Pending).then(|| uid.clone()))
            .and_then(|uid| {
                self.studies
                    .iter()
                    .flat_map(|s| s.series.iter())
                    .find(|se| se.series_instance_uid == uid)
                    .and_then(|s| s.instances.first())
                    .map(|inst| (uid, inst.path.clone()))
            });
        let Some((uid, path)) = next else {
            return;
        };
        match crate::dcm::thumbnail::load_thumbnail(&path, 96) {
            Ok(t) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [t.width as usize, t.height as usize],
                    &t.rgba,
                );
                let tex =
                    ctx.load_texture(format!("thumb-{uid}"), img, egui::TextureOptions::LINEAR);
                self.thumbnails.insert(uid, ThumbnailState::Ready(tex));
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "thumbnail failed");
                self.thumbnails.insert(uid, ThumbnailState::Failed);
            }
        }
        ctx.request_repaint();
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

/// Read which breast an MG instance shows. Used to set up the cell's
/// chest-wall anchor + mirror flag.
fn mg_side(inst: &crate::dcm::Instance) -> Option<MgSide> {
    match inst.image_laterality.as_deref()? {
        "R" => Some(MgSide::Right),
        "L" => Some(MgSide::Left),
        _ => None,
    }
}

fn apply_visuals(ctx: &Context, _dark: bool) {
    // The Workstation Noir theme is dark-only by design — light mode would
    // break the radiograph-glow accent that anchors the whole palette.
    crate::ui::theme::install(ctx);
}

/// Lifecycle of a sidebar series thumbnail.
pub enum ThumbnailState {
    Pending,
    Ready(TextureHandle),
    Failed,
}

impl eframe::App for DicomViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // CRITICAL: apply drops queued by last frame *before* rendering.
        // See `pending_drops` doc comment — dropping a TextureHandle
        // mid-frame triggers a wgpu "texture destroyed" panic.
        self.flush_pending_drops();
        self.handle_dropped_files(ctx);
        // Auto-load on first frame so the empty UI shows for one tick
        // before the (potentially CD-slow) folder scan begins.
        if let Some(folder) = self.pending_startup.take() {
            self.last_info = Some(format!("Loading {} …", folder.display()));
            tracing::info!(path = %folder.display(), "autoload startup folder");
            self.open_folder(&folder);
            ctx.request_repaint();
        }
        ui::draw(ctx, self);
        // After the UI registered any new Pending thumbnails, decode at
        // most one this frame and request a repaint if there's more work.
        self.pump_thumbnails(ctx);
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.config.save(&self.paths) {
            tracing::warn!(error = %e, "failed to save config on exit");
        }
    }
}

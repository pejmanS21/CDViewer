//! Top-level egui application: holds shared state and drives the UI.
//!
//! [`DicomViewerApp`] is the single [`eframe::App`] for the viewer.
//! Everything UI-visible — loaded studies, decoded pixel caches, the
//! viewport grid, annotations, transient error/info banners — lives on
//! this struct. The application is single-threaded; decode runs on the UI
//! thread and we lean on lazy caching plus per-frame work limits
//! (e.g. `pump_thumbnails`) to keep frame times low.

use crate::config::{Config, Paths};
use crate::dcm::annotation::{Annotation, AnnotationStore};
use crate::dcm::{self, metadata::TagRow, RawImage, Study};
use crate::ui::{self, GridLayout};
use eframe::CreationContext;
use egui::{Context, TextureHandle, Vec2};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Byte budget for [`DicomViewerApp::raw_cache`] pixel data before the
/// oldest entry is evicted. Decoded frames vary hugely — a 512×512 CT
/// slice is ~1 MB of `f32`, but a decimated mammogram at
/// `MAX_DISPLAY_DIM` (2048 px/side) is ~16 MB — so the bound is on bytes,
/// not entry count: an entry cap would let full-res stacks blow past any
/// memory target on the low-RAM Windows machines this viewer ships to.
const RAW_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;

/// The single eframe application.
///
/// Created by [`Self::new`] from [`main`](../main/index.html), then handed
/// to [`eframe::run_native`]. Every per-frame operation goes through
/// the `eframe::App::update` method below.
pub struct DicomViewerApp {
    /// Resolved data/log/exe paths. See [`Paths::resolve`].
    pub paths: Paths,
    /// Loaded `config.toml` (or defaults on first run).
    pub config: Config,
    /// Transient UI state — disclaimer/about dialogs, active tool, panel
    /// visibility, in-progress measurement, etc.
    pub ui_state: ui::UiState,

    /// All loaded studies (metadata only; pixels live in [`Self::raw_cache`]).
    pub studies: Vec<Study>,
    /// Decoded pixel buffers, keyed by SOP Instance UID. Filled lazily by
    /// [`Self::raw_for`]; cleared on [`Self::open_folder`]. Bounded to
    /// [`RAW_CACHE_MAX_BYTES`] of pixel data (oldest evicted first) so
    /// scrolling a long CT/MR stack can't grow this without limit on
    /// low-RAM Windows targets.
    pub raw_cache: HashMap<String, Arc<RawImage>>,
    /// Insertion order for [`Self::raw_cache`], oldest first. Drives FIFO
    /// eviction in [`Self::raw_for`].
    raw_cache_order: VecDeque<String>,
    /// Total bytes of `values` held in [`Self::raw_cache`]; compared
    /// against [`RAW_CACHE_MAX_BYTES`] to drive eviction.
    raw_cache_bytes: usize,
    /// SOP UIDs whose decode failed and the reason — so we don't retry
    /// every frame.
    pub raw_failures: HashMap<String, String>,
    /// Flattened DICOM tag rows for the metadata panel, keyed by SOP UID.
    /// Lazily populated by [`Self::metadata_for`].
    pub metadata_cache: HashMap<String, Arc<Vec<TagRow>>>,
    /// Substring filter applied to the metadata panel.
    pub metadata_filter: String,
    /// Sidebar previews. Lazily populated one-per-frame so the UI never
    /// stalls behind a chain of pixel decodes.
    pub thumbnails: HashMap<String, ThumbnailState>,

    /// Active grid layout (1×1 up to 4×4).
    pub grid: GridLayout,
    /// One [`CellState`] per visible viewport cell. Length matches
    /// `grid.cell_count()`.
    pub cells: Vec<CellState>,
    /// Index into [`Self::cells`] receiving toolbar actions and keyboard
    /// input.
    pub active_cell: usize,

    /// Per-SOP-UID annotation list, persisted to a JSON sidecar.
    pub annotation_store: AnnotationStore,
    /// Where [`Self::annotation_store`] is loaded from / saved to.
    pub annotations_path: PathBuf,

    /// Last error message — shown in the status bar.
    pub last_error: Option<String>,
    /// Last informational message — shown in the status bar.
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

/// All viewport state belongs to a cell, so transforms persist when
/// switching layouts and so dragging a different series onto a cell
/// resets only that cell.
///
/// **Coordinate spaces:** [`Self::pan`] is in screen pixels;
/// [`Self::zoom`] is a unitless scale; rotation is in quarter turns.
/// Window/level applies to the rescaled pixel values held in
/// [`RawImage::values`](crate::dcm::pixel::RawImage::values).
#[derive(Clone)]
pub struct CellState {
    /// `(study_idx, series_idx)` into [`DicomViewerApp::studies`]. `None`
    /// for an empty cell.
    pub series_ref: Option<(usize, usize)>,
    /// Index into the series' `instances` for the slice on screen.
    pub slice: usize,
    /// Pan offset in screen pixels.
    pub pan: Vec2,
    /// Display zoom (1.0 = fit to cell).
    pub zoom: f32,
    /// Quarter-turn rotation count (0, 1, 2, 3).
    pub rotation_quarter: u8,
    /// Horizontal flip.
    pub flip_h: bool,
    /// Vertical flip.
    pub flip_v: bool,
    /// Photometric inversion *requested by the user*. XOR'd with the
    /// instance's intrinsic invert (MONOCHROME1) at render time.
    pub invert: bool,
    /// `(center, width)` override. `None` = use the auto window.
    pub window: Option<(f64, f64)>,
    /// Decode/render error for this cell, surfaced in the cell overlay.
    pub error: Option<String>,
    /// Cached uploaded texture, keyed by `(tex_uid, tex_window, tex_invert)`.
    pub tex: Option<TextureHandle>,
    /// SOP UID the cached texture was rendered from.
    pub tex_uid: Option<String>,
    /// Window/level the cached texture was rendered with.
    pub tex_window: Option<(f64, f64)>,
    /// Whether the cached texture has user-invert applied.
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
    /// Reset pan/zoom/rotation/flip/invert/window to defaults. The cached
    /// texture is left in place; the renderer notices the mismatch via
    /// the `(tex_uid, tex_window, tex_invert)` comparison and regenerates.
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

    /// Bind this cell to a series and slice, discarding all previous
    /// transform/window state. Used when a series is dragged onto a cell.
    pub fn assign(&mut self, series_ref: (usize, usize), slice: usize) {
        *self = CellState::default();
        self.series_ref = Some(series_ref);
        self.slice = slice;
    }
}

impl DicomViewerApp {
    /// Build the app, install the theme, and load annotations from
    /// `<data_dir>/annotations.json`.
    ///
    /// `startup_folder` defers the actual folder scan to the first
    /// [`update`](eframe::App::update) tick so the window paints before
    /// the (potentially CD-slow) scan begins.
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
            raw_cache_order: VecDeque::new(),
            raw_cache_bytes: 0,
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

    /// Persist [`Self::annotation_store`] to [`Self::annotations_path`].
    /// Failures are logged but not propagated — losing one annotation save
    /// shouldn't crash the viewer.
    pub fn save_annotations(&self) {
        if let Err(e) = self.annotation_store.save(&self.annotations_path) {
            tracing::warn!(error = %e, "annotation save failed");
        }
    }

    /// Append `ann` to the list for `sop_uid` and immediately save.
    pub fn push_annotation(&mut self, sop_uid: &str, ann: Annotation) {
        self.annotation_store.push(sop_uid, ann);
        self.save_annotations();
    }

    /// Drop every annotation on the instance currently shown by the active
    /// cell and immediately save.
    pub fn clear_annotations_for_active(&mut self) {
        if let Some(inst) = self.active_instance().cloned() {
            self.annotation_store.clear_instance(&inst.sop_instance_uid);
            self.save_annotations();
        }
    }

    /// Remove the most recently pushed annotation on the active cell's
    /// instance and immediately save.
    pub fn undo_last_annotation_for_active(&mut self) {
        if let Some(inst) = self.active_instance().cloned() {
            self.annotation_store.pop_last(&inst.sop_instance_uid);
            self.save_annotations();
        }
    }

    /// Switch grid layout, resizing [`Self::cells`] to match. Existing
    /// cells are preserved in array order; extras (when shrinking) are
    /// truncated. Resets [`Self::active_cell`] if it now falls out of range.
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

    /// Scan a folder for DICOM files, replace [`Self::studies`], and
    /// auto-arrange the first one. Clears every per-instance cache (raw
    /// pixels, metadata rows, thumbnails) and every [`CellState`]. Errors
    /// are surfaced via [`Self::last_error`] for the status bar — they're
    /// not propagated to the caller.
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
                self.raw_cache_order.clear();
                self.raw_cache_bytes = 0;
                self.raw_failures.clear();
                self.metadata_cache.clear();
                self.thumbnails.clear();
                for c in &mut self.cells {
                    *c = CellState::default();
                }
                // Auto-arrange: try MG hanging protocol across the whole
                // study first (handles 4-series-of-1-instance layouts that
                // the per-series ≥4 check in `select_series` would miss),
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

    /// Convenience: open the parent folder of a single file. We always
    /// load a *folder* — there is no single-file mode.
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

    /// Look across every MG series in the study and assemble a 2×2 layout
    /// when there's at least one right-breast and one left-breast view.
    ///
    /// Placement priority:
    /// 1. Strict — RCC, LCC, RMLO, LMLO all present: place in that order.
    /// 2. View-known — at least 2 R and 2 L with `ViewPosition` set: sort
    ///    each side's queue by view (CC before MLO) so the top row is the
    ///    cranio-caudal pair.
    /// 3. Laterality-only — at least 2 R and 2 L but `ViewPosition` is
    ///    missing (common in some PACS exports): pair in series/instance
    ///    order.
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
                let side = match inst.image_laterality.as_deref() {
                    Some("R") => &mut rights,
                    Some("L") => &mut lefts,
                    _ => continue,
                };
                let vp = match inst.view_position.as_deref() {
                    Some("CC") => 0u8,
                    Some("MLO") => 1u8,
                    _ => 2u8,
                };
                side.push((vp, si, ii));
            }
        }
        if rights.len() < 2 || lefts.len() < 2 {
            return false;
        }
        rights.sort_by_key(|&(vp, si, ii)| (vp, si, ii));
        lefts.sort_by_key(|&(vp, si, ii)| (vp, si, ii));

        self.set_grid(GridLayout::TwoByTwo);
        let pick = |v: &[(u8, usize, usize)], idx: usize| (v[idx].1, v[idx].2);
        let (r0_si, r0_ii) = pick(&rights, 0);
        let (l0_si, l0_ii) = pick(&lefts, 0);
        let (r1_si, r1_ii) = pick(&rights, 1);
        let (l1_si, l1_ii) = pick(&lefts, 1);

        self.cells[0].assign((study_idx, r0_si), r0_ii);
        self.cells[1].assign((study_idx, l0_si), l0_ii);
        self.cells[2].assign((study_idx, r1_si), r1_ii);
        self.cells[3].assign((study_idx, l1_si), l1_ii);
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

    /// Get the decoded [`RawImage`] for an instance, decoding on first
    /// miss. Returns `None` (and remembers the failure in
    /// [`Self::raw_failures`]) when decode fails, so the viewport doesn't
    /// re-attempt every frame.
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
                self.raw_cache_order.push_back(sop_uid.to_string());
                self.raw_cache_bytes += arc.values.len() * std::mem::size_of::<f32>();
                // ponytail: FIFO, not true LRU — good enough to bound memory
                // during linear slice scrolling; revisit if profiling shows
                // thrash on non-linear access patterns (e.g. jumping cells).
                // Always keep the newest entry, even if it alone exceeds
                // the budget — evicting what we just decoded would thrash.
                while self.raw_cache_bytes > RAW_CACHE_MAX_BYTES && self.raw_cache_order.len() > 1 {
                    if let Some(oldest) = self.raw_cache_order.pop_front() {
                        if let Some(evicted) = self.raw_cache.remove(&oldest) {
                            self.raw_cache_bytes = self
                                .raw_cache_bytes
                                .saturating_sub(evicted.values.len() * std::mem::size_of::<f32>());
                        }
                    }
                }
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

    /// Pick up files dropped onto the egui window. A dropped folder
    /// triggers [`Self::open_folder`]; a dropped file triggers
    /// [`Self::open_file`].
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

    /// Apply [`CellState::reset_view`] to the active cell.
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
    /// [`Self::pump_thumbnails`] decodes one at a time.
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

    /// Get the flattened metadata rows for an instance, parsing on first
    /// miss. Failures are logged but not cached, since metadata parsing is
    /// cheap and unlikely to repeat-fail.
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

fn apply_visuals(ctx: &Context, _dark: bool) {
    // The Workstation Noir theme is dark-only by design — light mode would
    // break the radiograph-glow accent that anchors the whole palette.
    crate::ui::theme::install(ctx);
}

/// Lifecycle of a sidebar series thumbnail.
///
/// New entries land as [`Self::Pending`] when the sidebar first asks for a
/// thumbnail. [`DicomViewerApp::pump_thumbnails`] decodes at most one per
/// frame, transitioning to [`Self::Ready`] or [`Self::Failed`].
pub enum ThumbnailState {
    /// Queued; the per-frame pump will decode this next.
    Pending,
    /// Decoded and uploaded as a GPU texture.
    Ready(TextureHandle),
    /// Decode failed; don't retry.
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

//! UI composition.
//!
//! The egui side of the viewer. [`draw`] is called once per frame from
//! [`DicomViewerApp::update`](crate::app::DicomViewerApp). It paints the
//! panels in this order:
//!
//! 1. `menu_bar` — top.
//! 2. `toolbar` — top, beneath the menu bar.
//! 3. `status_bar` — bottom.
//! 4. `study_browser` — left side panel (optional).
//! 5. `metadata_panel` — right side panel (optional).
//! 6. `viewport` — central area, multi-cell grid.
//! 7. Modal dialogs (`disclaimer`, `about`).
//!
//! ## Drag-and-drop quirks (egui 0.29)
//!
//! The sidebar uses `dnd_drag_source`. The inner `selectable_label`'s
//! response carries click sense (`inner.inner.clicked()`); the outer
//! response only has hover + drag. The viewport reads the payload via
//! `DragAndDrop::payload::<SeriesDragPayload>` and consumes it with
//! `clear_payload` on release. **Do not wrap cells in `dnd_drop_zone`** —
//! painted-not-allocated content makes the zone collapse.

mod dialogs;
mod menu_bar;
mod status_bar;
mod study_browser;
pub mod theme;
mod toolbar;
mod viewport;

use crate::app::DicomViewerApp;
use crate::config::Config;
use egui::Context;

/// Transient UI state — dialog visibility, active tool, panel visibility,
/// in-progress measurement. Persists across frames but not across runs
/// (use [`crate::config::UiConfig`] for the latter).
pub struct UiState {
    /// Show the "Not for diagnostic use" modal on this run.
    pub show_disclaimer: bool,
    /// Show the About modal on this run.
    pub show_about: bool,
    /// Show the right-hand metadata panel.
    pub show_metadata_panel: bool,
    /// Show the left-hand study browser.
    pub show_study_browser: bool,
    /// Toolbar selection.
    pub active_tool: ActiveTool,
    /// When `true`, draw existing annotations over the image.
    pub annotations_visible: bool,
    /// Mid-construction annotation (e.g. first click of a length measurement).
    pub in_progress: Option<InProgress>,
}

/// Click- or drag-based annotation under construction. All coords are in
/// displayed-image pixels (post-decimation).
///
/// Length and Angle are click-step state machines; Rect and Ellipse are
/// drag-state with continuously-updated `cur`.
#[derive(Debug, Clone, Copy)]
pub enum InProgress {
    /// First click of a Length measurement placed.
    LengthP1([f32; 2]),
    /// First click of an Angle measurement placed (p1).
    AngleP1([f32; 2]),
    /// First two clicks of an Angle measurement placed (p1, vertex).
    AngleP1V([f32; 2], [f32; 2]),
    /// Rectangle ROI being dragged.
    RectDrag {
        /// Where the drag started.
        start: [f32; 2],
        /// Current cursor position.
        cur: [f32; 2],
    },
    /// Ellipse ROI being dragged.
    EllipseDrag {
        /// Where the drag started.
        start: [f32; 2],
        /// Current cursor position.
        cur: [f32; 2],
    },
}

/// Which toolbar button is currently active. Drives mouse-drag behaviour
/// in the viewport. Window/Level, Pan, and Zoom are mutually exclusive
/// with the measurement tools; middle-mouse pan works in all modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTool {
    /// Drag to adjust window centre / width (default).
    WindowLevel,
    /// Drag to pan the image.
    Pan,
    /// Drag to zoom (down = in, up = out).
    Zoom,
    /// Two-click distance measurement.
    Length,
    /// Three-click angle measurement.
    Angle,
    /// Drag-to-place rectangle ROI.
    RectRoi,
    /// Drag-to-place ellipse ROI.
    EllipseRoi,
}

impl ActiveTool {
    /// `true` for Length / Angle / RectRoi / EllipseRoi — the tools that
    /// produce annotations.
    pub fn is_measurement(self) -> bool {
        matches!(
            self,
            Self::Length | Self::Angle | Self::RectRoi | Self::EllipseRoi
        )
    }
}

/// Viewport grid layout — number of columns × rows of cells.
///
/// The viewer supports layouts from 1×1 up to 4×4. Switching layouts
/// preserves existing [`crate::app::CellState`] entries in order and
/// truncates extras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLayout {
    /// Single cell.
    OneByOne,
    /// Two cells side by side.
    OneByTwo,
    /// Two cells stacked.
    TwoByOne,
    /// 2×2 — the MG hanging-protocol layout.
    TwoByTwo,
    /// Three cells side by side.
    OneByThree,
    /// Three cells stacked.
    ThreeByOne,
    /// 3 columns × 2 rows.
    TwoByThree,
    /// 2 columns × 3 rows.
    ThreeByTwo,
    /// 3×3 — useful for CT slabs.
    ThreeByThree,
    /// 4 columns × 2 rows.
    TwoByFour,
    /// 2 columns × 4 rows.
    FourByTwo,
    /// 4×4 — maximum supported.
    FourByFour,
}

impl GridLayout {
    /// Returns `(cols, rows)`.
    pub fn dims(self) -> (usize, usize) {
        match self {
            Self::OneByOne => (1, 1),
            Self::OneByTwo => (2, 1),
            Self::TwoByOne => (1, 2),
            Self::TwoByTwo => (2, 2),
            Self::OneByThree => (3, 1),
            Self::ThreeByOne => (1, 3),
            Self::TwoByThree => (3, 2),
            Self::ThreeByTwo => (2, 3),
            Self::ThreeByThree => (3, 3),
            Self::TwoByFour => (4, 2),
            Self::FourByTwo => (2, 4),
            Self::FourByFour => (4, 4),
        }
    }
    /// Total cell count = `cols * rows`.
    pub fn cell_count(self) -> usize {
        let (c, r) = self.dims();
        c * r
    }
    /// Short label for the toolbar dropdown (e.g. `"2×2"`).
    pub fn label(self) -> &'static str {
        match self {
            Self::OneByOne => "1×1",
            Self::OneByTwo => "1×2",
            Self::TwoByOne => "2×1",
            Self::TwoByTwo => "2×2",
            Self::OneByThree => "1×3",
            Self::ThreeByOne => "3×1",
            Self::TwoByThree => "2×3",
            Self::ThreeByTwo => "3×2",
            Self::ThreeByThree => "3×3",
            Self::TwoByFour => "2×4",
            Self::FourByTwo => "4×2",
            Self::FourByFour => "4×4",
        }
    }
    /// Every variant, in toolbar-display order.
    pub const ALL: [GridLayout; 12] = [
        Self::OneByOne,
        Self::OneByTwo,
        Self::TwoByOne,
        Self::TwoByTwo,
        Self::OneByThree,
        Self::ThreeByOne,
        Self::TwoByThree,
        Self::ThreeByTwo,
        Self::ThreeByThree,
        Self::TwoByFour,
        Self::FourByTwo,
        Self::FourByFour,
    ];
}

impl UiState {
    /// Construct initial UI state from the persisted [`Config`] and a
    /// disclaimer flag. The disclaimer modal is shown once per machine
    /// (until acknowledged).
    pub fn new(show_disclaimer: bool, cfg: &Config) -> Self {
        Self {
            show_disclaimer,
            show_about: false,
            show_metadata_panel: cfg.ui.show_metadata_panel,
            show_study_browser: cfg.ui.show_study_browser,
            active_tool: ActiveTool::WindowLevel,
            annotations_visible: true,
            in_progress: None,
        }
    }
}

/// Paint every UI panel for the current frame. Called once per frame
/// from the app's `update` method.
pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    menu_bar::draw(ctx, app);
    toolbar::draw(ctx, app);
    status_bar::draw(ctx, app);

    if app.ui_state.show_study_browser {
        study_browser::draw(ctx, app);
    }
    if app.ui_state.show_metadata_panel {
        dialogs::metadata_panel::draw(ctx, app);
    }

    viewport::draw(ctx, app);

    dialogs::disclaimer::draw(ctx, app);
    dialogs::about::draw(ctx, app);
}

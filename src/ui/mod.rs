//! UI composition.

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

pub struct UiState {
    pub show_disclaimer: bool,
    pub show_about: bool,
    pub show_metadata_panel: bool,
    pub show_study_browser: bool,
    pub active_tool: ActiveTool,
    pub annotations_visible: bool,
    /// Mid-construction annotation (e.g. first click of a length measurement).
    pub in_progress: Option<InProgress>,
}

/// Click- or drag-based annotation under construction. All coords are in
/// displayed-image pixels (post-decimation).
#[derive(Debug, Clone, Copy)]
pub enum InProgress {
    LengthP1([f32; 2]),
    AngleP1([f32; 2]),
    AngleP1V([f32; 2], [f32; 2]),
    RectDrag { start: [f32; 2], cur: [f32; 2] },
    EllipseDrag { start: [f32; 2], cur: [f32; 2] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTool {
    WindowLevel,
    Pan,
    Zoom,
    Length,
    Angle,
    RectRoi,
    EllipseRoi,
}

impl ActiveTool {
    pub fn is_measurement(self) -> bool {
        matches!(
            self,
            Self::Length | Self::Angle | Self::RectRoi | Self::EllipseRoi
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLayout {
    OneByOne,
    OneByTwo,
    TwoByOne,
    TwoByTwo,
    OneByThree,
    ThreeByOne,
    TwoByThree,
    ThreeByTwo,
    ThreeByThree,
    TwoByFour,
    FourByTwo,
    FourByFour,
}

impl GridLayout {
    /// Returns (cols, rows).
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
    pub fn cell_count(self) -> usize {
        let (c, r) = self.dims();
        c * r
    }
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

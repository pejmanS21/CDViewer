//! Workstation Noir theme — radiology instrument-panel aesthetic.
//!
//! Single source of truth for colours, strokes, spacing, and text styles.
//! No rounded corners. No gradients. One warm accent (amber) so the eye
//! always knows where the focus is.

use egui::{
    Color32, FontFamily, FontId, Margin, Rounding, Shadow, Stroke, Style, TextStyle, Visuals,
};

/// The signature amber. Warm enough to feel like a radiograph viewbox glow,
/// muted enough to live next to medical data without screaming.
pub const ACCENT: Color32 = Color32::from_rgb(0xE8, 0xA2, 0x2D);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x8E, 0x65, 0x1F);
pub const CYAN_DATA: Color32 = Color32::from_rgb(0x6F, 0xB3, 0xD2);

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x0D);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x0E, 0x10, 0x13);
pub const BG_DEEP: Color32 = Color32::from_rgb(0x05, 0x06, 0x07);
pub const BG_SURFACE: Color32 = Color32::from_rgb(0x14, 0x17, 0x1B);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x1C, 0x20, 0x25);

pub const FG: Color32 = Color32::from_rgb(0xE5, 0xE7, 0xEA);
pub const FG_DIM: Color32 = Color32::from_rgb(0x8A, 0x8E, 0x93);
pub const FG_FAINT: Color32 = Color32::from_rgb(0x55, 0x59, 0x5E);

pub const LINE: Color32 = Color32::from_rgb(0x20, 0x24, 0x29);
pub const LINE_BRIGHT: Color32 = Color32::from_rgb(0x33, 0x38, 0x3E);

pub const DANGER: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x6C);
pub const OK: Color32 = Color32::from_rgb(0x6C, 0xCF, 0x9A);

pub fn install(ctx: &egui::Context) {
    apply_visuals(ctx);
    apply_style(ctx);
}

fn apply_visuals(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    let r0: Rounding = Rounding::ZERO;

    v.dark_mode = true;
    v.override_text_color = Some(FG);

    v.window_fill = BG_PANEL;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.menu_rounding = r0;
    v.window_rounding = r0;
    v.window_highlight_topmost = false;

    v.panel_fill = BG;
    v.faint_bg_color = BG_SURFACE;
    v.extreme_bg_color = BG_DEEP;
    v.code_bg_color = BG_DEEP;

    v.hyperlink_color = ACCENT;
    v.warn_fg_color = ACCENT;
    v.error_fg_color = DANGER;

    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xE8, 0xA2, 0x2D, 60);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    // Widget palette: noninteractive < inactive < hovered < active.
    let mk = |bg: Color32, bg_stroke: Color32, fg_stroke: Color32| egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, bg_stroke),
        fg_stroke: Stroke::new(1.0, fg_stroke),
        rounding: r0,
        expansion: 0.0,
    };
    v.widgets.noninteractive = mk(BG_PANEL, LINE, FG_DIM);
    v.widgets.inactive = mk(BG_SURFACE, LINE, FG);
    v.widgets.hovered = mk(BG_HOVER, ACCENT_DIM, FG);
    v.widgets.active = mk(BG_HOVER, ACCENT, FG);
    v.widgets.open = mk(BG_HOVER, LINE_BRIGHT, FG);

    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = true;
    v.striped = false;

    ctx.set_visuals(v);
}

fn apply_style(ctx: &egui::Context) {
    let mut style: Style = (*ctx.style()).clone();

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    // Bigger button padding — tools and menus need a real click target.
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.menu_margin = Margin::symmetric(10.0, 8.0);
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.indent = 14.0;
    style.spacing.interact_size = egui::vec2(28.0, 28.0);
    style.spacing.combo_height = 240.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_inner_margin = 2.0;

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
    ]
    .into();

    style.animation_time = 0.10;
    style.interaction.show_tooltips_only_when_still = true;

    ctx.set_style(style);
}

/// Reusable: paint amber L-brackets at two diagonal corners. Used for the
/// active-viewport indicator — no full border, just a viewfinder frame.
pub fn paint_active_brackets(painter: &egui::Painter, rect: egui::Rect, color: Color32, arm: f32) {
    let s = Stroke::new(1.5, color);
    let r = rect.shrink(2.0);
    let tl = r.left_top();
    let br = r.right_bottom();

    painter.line_segment([tl, egui::pos2(tl.x + arm, tl.y)], s);
    painter.line_segment([tl, egui::pos2(tl.x, tl.y + arm)], s);
    painter.line_segment([br, egui::pos2(br.x - arm, br.y)], s);
    painter.line_segment([br, egui::pos2(br.x, br.y - arm)], s);
}

/// Hairline rule — the only kind of separator allowed in the theme.
pub fn hairline(ui: &mut egui::Ui, color: Color32) {
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, color),
    );
}

/// Modality → indicator colour. Each modality gets a stable hue so the
/// sidebar reads as a data table, not a list.
pub fn modality_color(modality: &str) -> Color32 {
    match modality.to_uppercase().as_str() {
        "CT" => Color32::from_rgb(0xE8, 0xA2, 0x2D),
        "MR" | "MRI" => Color32::from_rgb(0x9C, 0xB8, 0xE8),
        "MG" => Color32::from_rgb(0xE8, 0x8B, 0xB8),
        "US" => Color32::from_rgb(0x6C, 0xCF, 0x9A),
        "CR" | "DX" | "RG" => Color32::from_rgb(0xC8, 0xBE, 0xA0),
        "PT" | "PET" => Color32::from_rgb(0xD8, 0x7D, 0x7D),
        "XA" | "RF" => Color32::from_rgb(0xB0, 0xA0, 0xE0),
        _ => Color32::from_rgb(0x8A, 0x8E, 0x93),
    }
}

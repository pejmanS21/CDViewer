use crate::app::DicomViewerApp;
use crate::ui::theme;
use egui::{Context, FontId, Frame, Margin, RichText, Stroke, Window};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    if !app.ui_state.show_about {
        return;
    }
    let mut open = true;
    Window::new(RichText::new("ABOUT").monospace().color(theme::FG))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            Frame::default()
                .fill(theme::BG_PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .inner_margin(Margin::same(20.0)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            ui.label(
                RichText::new("DICOM VIEWER")
                    .font(FontId::monospace(14.0))
                    .color(theme::ACCENT),
            );
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .font(FontId::monospace(11.0))
                    .color(theme::FG_DIM),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("Portable · non-diagnostic")
                    .color(theme::FG)
                    .size(11.5),
            );
            ui.add_space(14.0);
            theme::hairline(ui, theme::LINE);
            ui.add_space(8.0);

            ui.label(
                RichText::new("EXE   ")
                    .font(FontId::monospace(10.5))
                    .color(theme::FG_FAINT),
            );
            ui.label(
                RichText::new(app.paths.exe_dir.display().to_string())
                    .font(FontId::monospace(10.5))
                    .color(theme::FG_DIM),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("DATA  ")
                    .font(FontId::monospace(10.5))
                    .color(theme::FG_FAINT),
            );
            ui.label(
                RichText::new(app.paths.data_dir.display().to_string())
                    .font(FontId::monospace(10.5))
                    .color(theme::FG_DIM),
            );
        });
    if !open {
        app.ui_state.show_about = false;
    }
}

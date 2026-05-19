use crate::app::DicomViewerApp;
use crate::ui::theme;
use egui::{Color32, Context, FontId, Frame, Margin, RichText, Stroke, Window};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    if !app.ui_state.show_disclaimer {
        return;
    }
    let mut acknowledged = false;
    Window::new("")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            Frame::default()
                .fill(theme::BG_PANEL)
                .stroke(Stroke::new(1.0, theme::ACCENT))
                .inner_margin(Margin::same(24.0)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            // Top accent bar.
            let (bar_rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 2.0), egui::Sense::hover());
            ui.painter().rect_filled(bar_rect, 0.0, theme::ACCENT);
            ui.add_space(14.0);

            ui.label(
                RichText::new("⚠  NOT FOR DIAGNOSTIC USE")
                    .font(FontId::monospace(13.0))
                    .color(theme::ACCENT),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "This viewer is intended for review and reference only.\n\
                     It is not a certified medical device.\n\
                     Do not use it for primary diagnosis.",
                )
                .color(theme::FG)
                .size(12.0),
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn = egui::Button::new(
                        RichText::new("I UNDERSTAND")
                            .font(FontId::monospace(11.5))
                            .color(theme::BG),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::new(1.0, theme::ACCENT));
                    if ui.add_sized([130.0, 28.0], btn).clicked() {
                        acknowledged = true;
                    }
                });
            });
        });

    if acknowledged {
        app.ui_state.show_disclaimer = false;
        app.config.disclaimer_acknowledged = true;
        if let Err(e) = app.config.save(&app.paths) {
            tracing::warn!(error = %e, "failed to persist disclaimer acknowledgement");
        }
    }
    let _ = Color32::WHITE;
}

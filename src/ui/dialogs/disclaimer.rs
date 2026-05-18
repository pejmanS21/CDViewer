use crate::app::DicomViewerApp;
use egui::{Context, Window};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    if !app.ui_state.show_disclaimer {
        return;
    }
    let mut acknowledged = false;
    Window::new("Not for diagnostic use")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                "This viewer is intended for review and reference only.\n\
                 It is NOT a certified medical device and must not be used \
                 for primary diagnosis.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("I understand").clicked() {
                    acknowledged = true;
                }
            });
        });

    if acknowledged {
        app.ui_state.show_disclaimer = false;
        app.config.disclaimer_acknowledged = true;
        if let Err(e) = app.config.save(&app.paths) {
            tracing::warn!(error = %e, "failed to persist disclaimer acknowledgement");
        }
    }
}

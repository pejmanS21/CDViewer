use crate::app::DicomViewerApp;
use egui::{Context, TopBottomPanel};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Ready");
            ui.separator();
            ui.label(if app.paths.is_portable() {
                "Portable mode"
            } else {
                "User-data mode"
            });
            ui.separator();
            ui.weak("Not for diagnostic use");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
            });
        });
    });
}

//! Bottom status bar.
//!
//! Shows the last error / info message, the loaded folder, the active
//! cell, and the portable-vs-user-data storage mode. Painted with the
//! [`crate::ui::theme`] palette.

use crate::app::DicomViewerApp;
use crate::ui::theme;
use egui::{Color32, Context, FontId, Frame, Margin, RichText, Stroke, TopBottomPanel};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    TopBottomPanel::bottom("status_bar")
        .frame(
            Frame::default()
                .fill(theme::BG_DEEP)
                .stroke(Stroke::new(1.0_f32, theme::LINE))
                .inner_margin(Margin::symmetric(10.0, 4.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mono_sm = FontId::monospace(10.5);

                // Live state on the left.
                if let Some(err) = &app.last_error {
                    ui.label(
                        RichText::new("●  ERR  ")
                            .font(mono_sm.clone())
                            .color(theme::DANGER),
                    );
                    ui.label(RichText::new(err).font(mono_sm.clone()).color(theme::FG));
                } else if let Some(info) = &app.last_info {
                    ui.label(
                        RichText::new("●  OK  ")
                            .font(mono_sm.clone())
                            .color(theme::OK),
                    );
                    ui.label(
                        RichText::new(info)
                            .font(mono_sm.clone())
                            .color(theme::FG_DIM),
                    );
                } else {
                    ui.label(
                        RichText::new("●  READY")
                            .font(mono_sm.clone())
                            .color(theme::ACCENT),
                    );
                }

                seg(ui);
                ui.label(
                    RichText::new(if app.paths.is_portable() {
                        "PORTABLE"
                    } else {
                        "USER-DATA"
                    })
                    .font(mono_sm.clone())
                    .color(theme::FG_DIM),
                );
                seg(ui);
                ui.label(
                    RichText::new(format!(
                        "STUDIES {:>2}    CELL {:>2}/{:<2}",
                        app.studies.len(),
                        app.active_cell + 1,
                        app.cells.len()
                    ))
                    .font(mono_sm.clone())
                    .color(theme::FG_DIM),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .font(mono_sm.clone())
                            .color(theme::FG_FAINT),
                    );
                    seg(ui);
                    ui.label(
                        RichText::new("NON-DIAGNOSTIC")
                            .font(mono_sm.clone())
                            .color(theme::ACCENT),
                    );
                });
            });
        });
    let _ = Color32::WHITE;
}

fn seg(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.label(RichText::new("·").color(theme::FG_FAINT).monospace());
    ui.add_space(6.0);
}

use crate::app::DicomViewerApp;
use egui::{Context, Id, SidePanel};

/// Payload carried by sidebar drag sources: (study_idx, series_idx).
pub type SeriesDragPayload = (usize, usize);

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    SidePanel::left("study_browser")
        .resizable(true)
        .default_width(260.0)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Studies");
            ui.separator();
            if app.studies.is_empty() {
                ui.weak("No studies loaded.");
                ui.add_space(6.0);
                ui.label("File ▸ Open Folder…");
                ui.label("Drag a folder onto the window.");
                ui.add_space(6.0);
                ui.weak("Tip: drag a series from this panel onto a viewport cell.");
                return;
            }

            let studies = app.studies.clone();
            let mut clicked: Option<(usize, usize)> = None;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (si, study) in studies.iter().enumerate() {
                    let open = app
                        .cells
                        .get(app.active_cell)
                        .and_then(|c| c.series_ref)
                        .map(|(s, _)| s == si)
                        .unwrap_or(si == 0);
                    egui::CollapsingHeader::new(study.label())
                        .id_salt(format!("study-{si}"))
                        .default_open(open)
                        .show(ui, |ui| {
                            if !study.patient_id.is_empty() {
                                ui.weak(format!("ID: {}", study.patient_id));
                            }
                            if !study.study_description.is_empty() {
                                ui.weak(&study.study_description);
                            }
                            ui.add_space(4.0);
                            for (ri, series) in study.series.iter().enumerate() {
                                let selected = app
                                    .cells
                                    .get(app.active_cell)
                                    .and_then(|c| c.series_ref)
                                    == Some((si, ri));
                                let label = format!(
                                    "[{}] {} — {} img",
                                    if series.modality.is_empty() {
                                        "?"
                                    } else {
                                        &series.modality
                                    },
                                    if series.description.is_empty() {
                                        "(no description)"
                                    } else {
                                        &series.description
                                    },
                                    series.instances.len()
                                );

                                let drag_id = Id::new(("series-drag", si, ri));
                                let payload: SeriesDragPayload = (si, ri);
                                // Capture the inner widget's response so its
                                // click sense is preserved. The outer
                                // `dnd_drag_source` response only carries
                                // hover+drag sense, so reading clicks from
                                // it would never fire.
                                let inner = ui.dnd_drag_source(drag_id, payload, |ui| {
                                    ui.selectable_label(selected, &label)
                                });
                                if inner.inner.clicked() {
                                    clicked = Some((si, ri));
                                }
                                inner
                                    .response
                                    .on_hover_text("Click to load · Drag onto a viewport cell");
                            }
                        });
                }
            });

            if let Some((si, ri)) = clicked {
                app.select_series(si, ri);
            }
        });
}

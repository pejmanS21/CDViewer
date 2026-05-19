use crate::app::DicomViewerApp;
use crate::dcm::{self, auto_window};
use crate::ui::theme;
use egui::{Context, FontId, Frame, Margin, RichText, Stroke, TopBottomPanel};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    TopBottomPanel::top("menu_bar")
        .frame(
            Frame::default()
                .fill(theme::BG_DEEP)
                .stroke(Stroke::new(1.0, theme::LINE))
                .inner_margin(Margin::symmetric(10.0, 4.0)),
        )
        .show(ctx, |ui| {
            // Small wordmark on the left — gives the app a brand presence
            // without leaning on a logo image.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("◈ DICOM")
                        .font(FontId::monospace(12.0))
                        .color(theme::ACCENT),
                );
                ui.add_space(6.0);
                let _ = ui.allocate_exact_size(egui::vec2(1.0, 14.0), egui::Sense::hover());
                ui.painter().line_segment(
                    [
                        egui::pos2(ui.cursor().min.x - 6.0, ui.cursor().min.y),
                        egui::pos2(ui.cursor().min.x - 6.0, ui.cursor().min.y + 14.0),
                    ],
                    Stroke::new(1.0, theme::LINE),
                );
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open File…").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("DICOM", &["dcm", "dicom"])
                                .pick_file()
                            {
                                app.open_file(&p);
                            }
                            ui.close_menu();
                        }
                        if ui.button("Open Folder…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                app.open_folder(&p);
                            }
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button("Export", |ui| {
                            if ui.button("Current view → PNG…").clicked() {
                                export_current_view(app, false);
                                ui.close_menu();
                            }
                            if ui
                                .button("Current view → PNG (with annotations)…")
                                .clicked()
                            {
                                export_current_view(app, true);
                                ui.close_menu();
                            }
                            if ui.button("Current series → PNG sequence…").clicked() {
                                export_series_pngs(app, false);
                                ui.close_menu();
                            }
                            if ui
                                .button("Current series → PNG sequence (with annotations)…")
                                .clicked()
                            {
                                export_series_pngs(app, true);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Anonymize active study → folder…").clicked() {
                                anonymize_active_study(app);
                                ui.close_menu();
                            }
                        });
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut app.ui_state.show_study_browser, "Study browser");
                        ui.checkbox(&mut app.ui_state.show_metadata_panel, "Metadata panel");
                        ui.checkbox(&mut app.ui_state.annotations_visible, "Annotations");
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("About").clicked() {
                            app.ui_state.show_about = true;
                            ui.close_menu();
                        }
                    });
                });
            });
        });
}

fn export_current_view(app: &mut DicomViewerApp, burn: bool) {
    let Some(inst) = app.active_instance().cloned() else {
        app.last_error = Some("No image in active cell.".into());
        return;
    };
    let Some(raw) = app.raw_for(&inst.sop_instance_uid, &inst.path) else {
        app.last_error = Some("Failed to decode image.".into());
        return;
    };
    let cell = app.cells[app.active_cell].clone();
    let window = cell.window.unwrap_or_else(|| auto_window(&raw));
    let invert = cell.invert;

    let modality = current_modality(app).unwrap_or_default();
    let anns = app
        .annotation_store
        .for_instance(&inst.sop_instance_uid)
        .to_vec();

    let default_name = format!("dicom_view_{}.png", short_uid(&inst.sop_instance_uid));
    let Some(out) = rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .set_file_name(&default_name)
        .save_file()
    else {
        return;
    };

    match dcm::export::export_current_view(
        &inst, &raw, window, invert, &anns, burn, &out, &modality,
    ) {
        Ok(()) => {
            tracing::info!(out = %out.display(), "wrote png");
            app.last_info = Some(format!("Saved {}", out.display()));
        }
        Err(e) => {
            tracing::error!(error = %e, "export png failed");
            app.last_error = Some(format!("{e:#}"));
        }
    }
}

fn export_series_pngs(app: &mut DicomViewerApp, burn: bool) {
    let Some(series) = current_series(app).cloned() else {
        app.last_error = Some("No series in active cell.".into());
        return;
    };
    let Some(out_dir) = rfd::FileDialog::new()
        .set_title("Choose output folder")
        .pick_folder()
    else {
        return;
    };
    match dcm::export::export_series_pngs(&series, &out_dir, &app.annotation_store, burn) {
        Ok(n) => {
            tracing::info!(count = n, out = %out_dir.display(), "wrote png sequence");
            app.last_info = Some(format!("Wrote {n} files to {}", out_dir.display()));
        }
        Err(e) => {
            tracing::error!(error = %e, "export sequence failed");
            app.last_error = Some(format!("{e:#}"));
        }
    }
}

fn anonymize_active_study(app: &mut DicomViewerApp) {
    let Some(study) = current_study(app).cloned() else {
        app.last_error = Some("No study active.".into());
        return;
    };
    let Some(out_dir) = rfd::FileDialog::new()
        .set_title("Choose output folder for anonymized study")
        .pick_folder()
    else {
        return;
    };
    match dcm::export::anonymize_study(&study, &out_dir) {
        Ok((ok, fail)) => {
            tracing::info!(ok, fail, out = %out_dir.display(), "anonymize done");
            app.last_info = Some(format!(
                "Anonymized {ok} file(s){} → {}",
                if fail > 0 {
                    format!(" ({fail} failed)")
                } else {
                    String::new()
                },
                out_dir.display()
            ));
        }
        Err(e) => {
            tracing::error!(error = %e, "anonymize failed");
            app.last_error = Some(format!("{e:#}"));
        }
    }
}

fn current_modality(app: &DicomViewerApp) -> Option<String> {
    let (si, se) = app.cells.get(app.active_cell)?.series_ref?;
    Some(app.studies.get(si)?.series.get(se)?.modality.clone())
}

fn current_series(app: &DicomViewerApp) -> Option<&crate::dcm::Series> {
    let (si, se) = app.cells.get(app.active_cell)?.series_ref?;
    app.studies.get(si)?.series.get(se)
}

fn current_study(app: &DicomViewerApp) -> Option<&crate::dcm::Study> {
    let (si, _) = app.cells.get(app.active_cell)?.series_ref?;
    app.studies.get(si)
}

fn short_uid(uid: &str) -> String {
    uid.chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

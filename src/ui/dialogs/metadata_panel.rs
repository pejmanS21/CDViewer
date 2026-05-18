use crate::app::DicomViewerApp;
use crate::dcm::metadata::TagRow;
use egui::{Color32, Context, ScrollArea, SidePanel};
use std::collections::BTreeMap;
use std::sync::Arc;

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    SidePanel::right("metadata_panel")
        .resizable(true)
        .default_width(360.0)
        .min_width(260.0)
        .show(ctx, |ui| {
            ui.heading("DICOM Metadata");
            let inst = app.active_instance().cloned();
            let Some(inst) = inst else {
                ui.separator();
                ui.weak("No instance selected.");
                return;
            };
            let rows = app.metadata_for(&inst.sop_instance_uid, &inst.path);
            let Some(rows) = rows else {
                ui.separator();
                ui.colored_label(Color32::LIGHT_RED, "Failed to read metadata.");
                return;
            };

            ui.horizontal(|ui| {
                ui.label("Filter:");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut app.metadata_filter)
                        .hint_text("name, tag, or value"),
                );
                if resp.changed() {
                    // no-op; just keeps the field reactive
                }
                if ui.button("✕").on_hover_text("Clear filter").clicked() {
                    app.metadata_filter.clear();
                }
            });
            ui.weak(format!("{} tags", rows.len()));
            ui.separator();

            let filter = app.metadata_filter.to_lowercase();
            let filtered = filter_rows(&rows, &filter);
            let by_group = group_rows(&filtered);

            ScrollArea::vertical().show(ui, |ui| {
                for (group_label, group_rows) in by_group {
                    egui::CollapsingHeader::new(group_label)
                        .id_salt(format!("meta-grp-{}", group_rows[0].tag.0))
                        .default_open(true)
                        .show(ui, |ui| {
                            for row in group_rows {
                                draw_row(ui, row);
                            }
                        });
                }
            });
        });
}

fn filter_rows<'a>(rows: &'a [TagRow], filter: &str) -> Vec<&'a TagRow> {
    if filter.is_empty() {
        return rows.iter().collect();
    }
    rows.iter()
        .filter(|r| {
            r.name.to_lowercase().contains(filter)
                || r.tag_label().to_lowercase().contains(filter)
                || r.value.to_lowercase().contains(filter)
        })
        .collect()
}

fn group_rows<'a>(rows: &[&'a TagRow]) -> Vec<(String, Vec<&'a TagRow>)> {
    let mut map: BTreeMap<u16, (String, Vec<&'a TagRow>)> = BTreeMap::new();
    for r in rows {
        let entry = map
            .entry(r.tag.0)
            .or_insert_with(|| (r.group_label(), Vec::new()));
        entry.1.push(*r);
    }
    map.into_values().collect()
}

fn draw_row(ui: &mut egui::Ui, row: &TagRow) {
    let resp = ui.horizontal(|ui| {
        ui.monospace(row.tag_label());
        ui.weak(&row.vr);
        ui.label(&row.name);
    });
    ui.indent(format!("v-{}-{}", row.tag.0, row.tag.1), |ui| {
        ui.colored_label(Color32::from_rgb(160, 200, 255), &row.value);
    });
    resp.response.context_menu(|ui| {
        if ui.button("Copy value").clicked() {
            ui.ctx().copy_text(row.value.clone());
            ui.close_menu();
        }
        if ui.button("Copy tag").clicked() {
            ui.ctx().copy_text(row.tag_label());
            ui.close_menu();
        }
        if ui.button("Copy name").clicked() {
            ui.ctx().copy_text(row.name.clone());
            ui.close_menu();
        }
    });
    ui.add_space(2.0);
    // Unused suppression for Arc symmetry.
    let _ = Arc::new(());
}

use crate::app::DicomViewerApp;
use crate::dcm::metadata::TagRow;
use crate::ui::theme;
use egui::{Color32, Context, FontId, Frame, Margin, RichText, ScrollArea, SidePanel, Stroke};
use std::collections::BTreeMap;

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    SidePanel::right("metadata_panel")
        .resizable(true)
        .default_width(380.0)
        .min_width(280.0)
        .frame(
            Frame::default()
                .fill(theme::BG_PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .inner_margin(Margin::symmetric(10.0, 10.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("METADATA")
                        .font(FontId::monospace(11.0))
                        .color(theme::FG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let inst = app.active_instance().cloned();
                    if let Some(inst) = inst.as_ref() {
                        ui.label(
                            RichText::new(short(&inst.sop_instance_uid))
                                .font(FontId::monospace(10.0))
                                .color(theme::FG_FAINT),
                        );
                    }
                });
            });
            theme::hairline(ui, theme::LINE);
            ui.add_space(6.0);

            let inst = app.active_instance().cloned();
            let Some(inst) = inst else {
                ui.label(
                    RichText::new("◇  no instance selected")
                        .font(FontId::monospace(11.0))
                        .color(theme::FG_FAINT),
                );
                return;
            };
            let rows = app.metadata_for(&inst.sop_instance_uid, &inst.path);
            let Some(rows) = rows else {
                ui.label(
                    RichText::new("●  failed to read metadata")
                        .font(FontId::monospace(11.0))
                        .color(theme::DANGER),
                );
                return;
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("FILTER")
                        .font(FontId::monospace(9.5))
                        .color(theme::FG_FAINT),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.metadata_filter)
                        .hint_text("name · tag · value")
                        .desired_width(f32::INFINITY)
                        .font(FontId::monospace(11.0)),
                );
            });

            let filter = app.metadata_filter.to_lowercase();
            let filtered = filter_rows(&rows, &filter);

            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{:>4} of {:>4} tag(s)", filtered.len(), rows.len()))
                    .font(FontId::monospace(10.5))
                    .color(theme::FG_DIM),
            );
            ui.add_space(6.0);

            let by_group = group_rows(&filtered);

            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for (group_label, group_rows) in by_group {
                        group_header(ui, &group_label, group_rows.len());
                        for row in group_rows {
                            draw_row(ui, row);
                        }
                        ui.add_space(6.0);
                    }
                });
        });
}

fn group_header(ui: &mut egui::Ui, label: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(10.5))
                .color(theme::ACCENT),
        );
        ui.label(
            RichText::new(format!("[{count}]"))
                .font(FontId::monospace(10.0))
                .color(theme::FG_FAINT),
        );
    });
    theme::hairline(ui, theme::LINE);
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
        ui.label(
            RichText::new(row.tag_label())
                .font(FontId::monospace(10.5))
                .color(theme::FG_DIM),
        );
        ui.label(
            RichText::new(&row.vr)
                .font(FontId::monospace(10.0))
                .color(theme::FG_FAINT),
        );
        ui.label(
            RichText::new(&row.name)
                .font(FontId::proportional(11.5))
                .color(theme::FG),
        );
    });
    ui.indent(format!("v-{}-{}", row.tag.0, row.tag.1), |ui| {
        ui.label(
            RichText::new(&row.value)
                .font(FontId::monospace(11.0))
                .color(theme::CYAN_DATA),
        );
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
    ui.add_space(3.0);
    let _ = Color32::WHITE;
}

fn short(uid: &str) -> String {
    let tail: String = uid
        .chars()
        .rev()
        .take(12)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}

use crate::app::DicomViewerApp;
use crate::ui::theme;
use egui::{Color32, Context, FontId, Id, Rect, SidePanel, Stroke, Vec2};

/// Payload carried by sidebar drag sources: (study_idx, series_idx).
pub type SeriesDragPayload = (usize, usize);

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    SidePanel::left("study_browser")
        .resizable(true)
        .default_width(264.0)
        .min_width(220.0)
        .frame(
            egui::Frame::default()
                .fill(theme::BG_PANEL)
                .inner_margin(egui::Margin::symmetric(10.0, 10.0)),
        )
        .show(ctx, |ui| {
            header(ui, app.studies.len());
            theme::hairline(ui, theme::LINE);
            ui.add_space(8.0);

            if app.studies.is_empty() {
                empty_state(ui);
                return;
            }

            let studies = app.studies.clone();
            let mut clicked: Option<(usize, usize)> = None;

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for (si, study) in studies.iter().enumerate() {
                        let open = app
                            .cells
                            .get(app.active_cell)
                            .and_then(|c| c.series_ref)
                            .map(|(s, _)| s == si)
                            .unwrap_or(si == 0);

                        ui.add_space(2.0);
                        egui::CollapsingHeader::new(
                            egui::RichText::new(study.label())
                                .color(theme::FG)
                                .monospace()
                                .size(11.5),
                        )
                        .id_salt(format!("study-{si}"))
                        .default_open(open)
                        .show(ui, |ui| {
                            if !study.patient_id.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!("ID  {}", study.patient_id))
                                        .monospace()
                                        .color(theme::FG_DIM)
                                        .size(10.5),
                                );
                            }
                            if !study.study_description.is_empty() {
                                ui.label(
                                    egui::RichText::new(&study.study_description)
                                        .color(theme::FG_DIM)
                                        .size(10.5),
                                );
                            }
                            ui.add_space(4.0);

                            for (ri, series) in study.series.iter().enumerate() {
                                let selected =
                                    app.cells.get(app.active_cell).and_then(|c| c.series_ref)
                                        == Some((si, ri));

                                let thumb = app.thumbnail_for(&series.series_instance_uid);

                                let drag_id = Id::new(("series-drag", si, ri));
                                let payload: SeriesDragPayload = (si, ri);
                                let inner = ui.dnd_drag_source(drag_id, payload, |ui| {
                                    series_row(
                                        ui,
                                        &series.modality,
                                        &series.description,
                                        series.instances.len(),
                                        series.series_number,
                                        selected,
                                        thumb.as_ref(),
                                    )
                                });
                                if inner.inner.clicked() {
                                    clicked = Some((si, ri));
                                }
                                inner
                                    .response
                                    .on_hover_text("Click to load · drag onto a viewport cell");
                            }
                        });
                    }
                });

            if let Some((si, ri)) = clicked {
                app.select_series(si, ri);
            }
        });
}

fn header(ui: &mut egui::Ui, n: usize) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("STUDIES")
                .monospace()
                .color(theme::FG)
                .size(11.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("[{n}]"))
                    .monospace()
                    .color(theme::ACCENT)
                    .size(11.0),
            );
        });
    });
}

fn empty_state(ui: &mut egui::Ui) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("◇")
                .color(theme::LINE_BRIGHT)
                .size(24.0),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("NO STUDIES LOADED")
                .monospace()
                .color(theme::FG_DIM)
                .size(11.0),
        );
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("File ▸ Open Folder…")
                .color(theme::FG)
                .size(11.0),
        );
        ui.label(
            egui::RichText::new("or drop a folder onto the window")
                .color(theme::FG_FAINT)
                .size(10.5),
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn series_row(
    ui: &mut egui::Ui,
    modality: &str,
    description: &str,
    count: usize,
    series_number: i32,
    selected: bool,
    thumbnail: Option<&egui::TextureHandle>,
) -> egui::Response {
    let modality = if modality.is_empty() { "??" } else { modality };
    let desc = if description.is_empty() {
        "(no description)"
    } else {
        description
    };

    let full_width = ui.available_width();
    let row_height = 62.0;
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(full_width, row_height), egui::Sense::click());

    let painter = ui.painter_at(rect);

    // Background for selected / hovered.
    let bg = if selected {
        theme::BG_SURFACE
    } else if resp.hovered() {
        theme::BG_HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        painter.rect_filled(rect, 0.0, bg);
    }

    // Bottom hairline so rows feel like records, not floating chips.
    painter.line_segment(
        [
            rect.left_bottom() + Vec2::new(0.0, -0.5),
            rect.right_bottom() + Vec2::new(0.0, -0.5),
        ],
        Stroke::new(1.0, theme::LINE),
    );

    // Left vertical accent bar when selected.
    if selected {
        painter.line_segment(
            [
                rect.left_top() + Vec2::new(1.0, 2.0),
                rect.left_bottom() + Vec2::new(1.0, -2.0),
            ],
            Stroke::new(2.0, theme::ACCENT),
        );
    }

    // Thumbnail slot (square, 48×48 inside a 1px deep frame).
    let thumb_size = row_height - 14.0;
    let thumb_rect = Rect::from_min_size(
        rect.left_top() + Vec2::new(8.0, (row_height - thumb_size) / 2.0),
        Vec2::new(thumb_size, thumb_size),
    );
    painter.rect_filled(thumb_rect, 0.0, theme::BG_DEEP);
    painter.rect_stroke(thumb_rect, 0.0, Stroke::new(1.0, theme::LINE));

    if let Some(tex) = thumbnail {
        let img_size = tex.size_vec2();
        let scale = (thumb_rect.width() / img_size.x).min(thumb_rect.height() / img_size.y);
        let draw = Vec2::new(img_size.x * scale, img_size.y * scale);
        let center = thumb_rect.center();
        let img_rect = Rect::from_center_size(center, draw);
        painter.image(
            tex.id(),
            img_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        // Tiny shimmer dot so empty thumbs read as "loading", not "broken".
        painter.text(
            thumb_rect.center(),
            egui::Align2::CENTER_CENTER,
            "◌",
            FontId::monospace(14.0),
            theme::FG_FAINT,
        );
    }

    // Modality chip sits at the top of the right column.
    let chip_w = 38.0;
    let chip_h = 16.0;
    let col_left = thumb_rect.right() + 10.0;
    let chip_rect = Rect::from_min_size(
        egui::pos2(col_left, rect.top() + 8.0),
        Vec2::new(chip_w, chip_h),
    );
    painter.rect_filled(chip_rect, 0.0, theme::modality_color(modality));
    painter.text(
        chip_rect.center(),
        egui::Align2::CENTER_CENTER,
        modality,
        FontId::monospace(11.0),
        theme::BG,
    );

    // Series number badge — small, dim, after the chip.
    let series_label = format!("#{series_number:>3}");
    painter.text(
        egui::pos2(chip_rect.right() + 6.0, chip_rect.center().y),
        egui::Align2::LEFT_CENTER,
        series_label,
        FontId::monospace(11.0),
        theme::FG_DIM,
    );

    // Right-aligned image count.
    let count_label = format!("{count} img");
    painter.text(
        egui::pos2(rect.right() - 8.0, chip_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        count_label,
        FontId::monospace(11.0),
        theme::ACCENT,
    );

    // Description on its own line below — single line, truncated.
    let desc_y = chip_rect.bottom() + 6.0;
    let max_w = (rect.right() - col_left - 8.0).max(20.0);
    let mut desc_text = desc.to_string();
    let mut galley =
        painter.layout_no_wrap(desc_text.clone(), FontId::proportional(12.5), theme::FG);
    while galley.size().x > max_w && desc_text.chars().count() > 4 {
        desc_text.pop();
        galley = painter.layout_no_wrap(
            format!("{desc_text}…"),
            FontId::proportional(12.5),
            theme::FG,
        );
    }
    painter.galley(egui::pos2(col_left, desc_y), galley, theme::FG);

    resp
}

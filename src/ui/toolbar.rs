use crate::app::DicomViewerApp;
use crate::ui::theme;
use crate::ui::{ActiveTool, GridLayout};
use egui::{Context, FontId, Frame, Margin, RichText, Stroke, TopBottomPanel};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    TopBottomPanel::top("toolbar")
        .frame(
            Frame::default()
                .fill(theme::BG_PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .inner_margin(Margin::symmetric(12.0, 8.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let prev_tool = app.ui_state.active_tool;

                group_label(ui, "VIEW");
                tool_button(
                    ui,
                    &mut app.ui_state.active_tool,
                    ActiveTool::WindowLevel,
                    "W/L",
                );
                tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Pan, "PAN");
                tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Zoom, "ZOOM");

                group_div(ui);
                group_label(ui, "MEASURE");
                tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Length, "LEN");
                tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Angle, "ANG");
                tool_button(
                    ui,
                    &mut app.ui_state.active_tool,
                    ActiveTool::RectRoi,
                    "ROI ▭",
                );
                tool_button(
                    ui,
                    &mut app.ui_state.active_tool,
                    ActiveTool::EllipseRoi,
                    "ROI ◯",
                );

                if app.ui_state.active_tool != prev_tool {
                    app.ui_state.in_progress = None;
                }

                group_div(ui);
                group_label(ui, "GRID");
                let mut grid = app.grid;
                egui::ComboBox::from_id_salt("grid-layout")
                    .selected_text(
                        RichText::new(grid.label())
                            .font(FontId::monospace(13.0))
                            .color(theme::FG),
                    )
                    .width(72.0)
                    .show_ui(ui, |ui| {
                        for opt in GridLayout::ALL {
                            ui.selectable_value(&mut grid, opt, opt.label());
                        }
                    });
                if grid != app.grid {
                    app.set_grid(grid);
                }

                let has_cell = app
                    .cells
                    .get(app.active_cell)
                    .is_some_and(|c| c.series_ref.is_some());

                group_div(ui);
                group_label(ui, "TRANSFORM");
                ui.add_enabled_ui(has_cell, |ui| {
                    if mini_btn(ui, "⟲")
                        .on_hover_text("Rotate counter-clockwise")
                        .clicked()
                    {
                        let c = &mut app.cells[app.active_cell];
                        c.rotation_quarter = (c.rotation_quarter + 3) % 4;
                    }
                    if mini_btn(ui, "⟳")
                        .on_hover_text("Rotate clockwise")
                        .clicked()
                    {
                        let c = &mut app.cells[app.active_cell];
                        c.rotation_quarter = (c.rotation_quarter + 1) % 4;
                    }
                    if mini_btn(ui, "⇄").on_hover_text("Flip horizontal").clicked() {
                        app.cells[app.active_cell].flip_h ^= true;
                    }
                    if mini_btn(ui, "⇅").on_hover_text("Flip vertical").clicked() {
                        app.cells[app.active_cell].flip_v ^= true;
                    }
                    if mini_btn(ui, "INV").on_hover_text("Invert lookup").clicked() {
                        app.cells[app.active_cell].invert ^= true;
                    }
                    if mini_btn(ui, "RESET")
                        .on_hover_text("Clear all view transforms")
                        .clicked()
                    {
                        app.reset_active_cell();
                    }
                });

                group_div(ui);
                group_label(ui, "PRESET");
                ui.menu_button(
                    RichText::new("WINDOW").font(FontId::monospace(12.5)),
                    |ui| {
                        let presets = [
                            ("Soft tissue", 40.0, 400.0),
                            ("Lung", -600.0, 1500.0),
                            ("Bone", 300.0, 1500.0),
                            ("Brain", 40.0, 80.0),
                            ("Abdomen", 60.0, 400.0),
                            ("Mediastinum", 50.0, 350.0),
                        ];
                        for (name, c, w) in presets {
                            let label = format!("{:<14}  C={:>5.0}  W={:>5.0}", name, c, w);
                            if ui
                                .add(egui::Button::new(
                                    RichText::new(label).font(FontId::monospace(12.5)),
                                ))
                                .clicked()
                            {
                                if let Some(cell) = app.cells.get_mut(app.active_cell) {
                                    cell.window = Some((c, w));
                                }
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new(
                                RichText::new("AUTO  (file default)").font(FontId::monospace(12.5)),
                            ))
                            .clicked()
                        {
                            if let Some(cell) = app.cells.get_mut(app.active_cell) {
                                cell.window = None;
                            }
                            ui.close_menu();
                        }
                    },
                );

                group_div(ui);
                group_label(ui, "ANN");
                ui.checkbox(
                    &mut app.ui_state.annotations_visible,
                    RichText::new("show").font(FontId::monospace(12.5)),
                );
                if mini_btn(ui, "UNDO")
                    .on_hover_text("Remove last annotation on this image")
                    .clicked()
                {
                    app.undo_last_annotation_for_active();
                }
                if mini_btn(ui, "CLEAR")
                    .on_hover_text("Remove every annotation on this image")
                    .clicked()
                {
                    app.clear_annotations_for_active();
                }
            });
        });
}

fn tool_button(ui: &mut egui::Ui, active: &mut ActiveTool, this: ActiveTool, label: &str) {
    let selected = *active == this;
    let hint = match this {
        ActiveTool::WindowLevel => "Drag: horizontal=width, vertical=center",
        ActiveTool::Pan => "Drag to pan (or middle-mouse)",
        ActiveTool::Zoom => "Drag up/down to zoom (or ⌘/Ctrl-scroll)",
        ActiveTool::Length => "Click 2 points — reports mm if pixel spacing present",
        ActiveTool::Angle => "Click 3 points: side · vertex · side",
        ActiveTool::RectRoi => "Drag a rectangle for mean/std/min/max",
        ActiveTool::EllipseRoi => "Drag an ellipse bbox for mean/std/min/max",
    };
    let text = RichText::new(label)
        .font(FontId::monospace(12.5))
        .color(if selected { theme::ACCENT } else { theme::FG });
    let btn = egui::SelectableLabel::new(selected, text);
    if ui.add(btn).on_hover_text(hint).clicked() {
        *active = this;
    }
}

fn mini_btn(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = RichText::new(label)
        .font(FontId::monospace(12.5))
        .color(theme::FG);
    ui.add(egui::Button::new(text).fill(theme::BG_SURFACE))
}

fn group_label(ui: &mut egui::Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .font(FontId::monospace(10.5))
            .color(theme::FG_FAINT),
    );
    ui.add_space(2.0);
}

fn group_div(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 16.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, theme::LINE),
    );
    ui.add_space(8.0);
}

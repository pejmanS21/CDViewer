use crate::app::DicomViewerApp;
use crate::ui::{ActiveTool, GridLayout};
use egui::{Context, TopBottomPanel};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let prev_tool = app.ui_state.active_tool;
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::WindowLevel, "W/L");
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Pan, "Pan");
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Zoom, "Zoom");
            ui.separator();
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Length, "📏 Len");
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::Angle, "∠ Ang");
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::RectRoi, "▭ ROI");
            tool_button(ui, &mut app.ui_state.active_tool, ActiveTool::EllipseRoi, "◯ ROI");
            if app.ui_state.active_tool != prev_tool {
                app.ui_state.in_progress = None;
            }
            ui.separator();

            // Grid layout selector.
            ui.label("Layout:");
            let mut grid = app.grid;
            egui::ComboBox::from_id_salt("grid-layout")
                .selected_text(grid.label())
                .show_ui(ui, |ui| {
                    for opt in GridLayout::ALL {
                        ui.selectable_value(&mut grid, opt, opt.label());
                    }
                });
            if grid != app.grid {
                app.set_grid(grid);
            }
            ui.separator();

            // Transform buttons act on the active cell.
            let has_cell = app
                .cells
                .get(app.active_cell)
                .is_some_and(|c| c.series_ref.is_some());

            ui.add_enabled_ui(has_cell, |ui| {
                if ui.button("⟲ 90°").on_hover_text("Rotate counter-clockwise").clicked() {
                    let c = &mut app.cells[app.active_cell];
                    c.rotation_quarter = (c.rotation_quarter + 3) % 4;
                }
                if ui.button("⟳ 90°").on_hover_text("Rotate clockwise").clicked() {
                    let c = &mut app.cells[app.active_cell];
                    c.rotation_quarter = (c.rotation_quarter + 1) % 4;
                }
                if ui.button("Flip H").clicked() {
                    app.cells[app.active_cell].flip_h ^= true;
                }
                if ui.button("Flip V").clicked() {
                    app.cells[app.active_cell].flip_v ^= true;
                }
                if ui.button("Invert").clicked() {
                    app.cells[app.active_cell].invert ^= true;
                }
                ui.separator();
                if ui.button("Reset View").on_hover_text("Clear pan/zoom/rotation/window").clicked() {
                    app.reset_active_cell();
                }
            });

            ui.separator();
            // Annotation controls.
            ui.checkbox(&mut app.ui_state.annotations_visible, "Show anns");
            if ui.button("Undo last").on_hover_text("Remove the last annotation on this image").clicked() {
                app.undo_last_annotation_for_active();
            }
            if ui.button("Clear all").on_hover_text("Remove every annotation on this image").clicked() {
                app.clear_annotations_for_active();
            }

            ui.separator();
            // Window/level presets — quick wins for radiology.
            ui.menu_button("Presets", |ui| {
                let presets = [
                    ("Soft Tissue", 40.0, 400.0),
                    ("Lung", -600.0, 1500.0),
                    ("Bone", 300.0, 1500.0),
                    ("Brain", 40.0, 80.0),
                    ("Abdomen", 60.0, 400.0),
                    ("Mediastinum", 50.0, 350.0),
                ];
                for (name, c, w) in presets {
                    if ui.button(format!("{name}  C={c}/W={w}")).clicked() {
                        if let Some(cell) = app.cells.get_mut(app.active_cell) {
                            cell.window = Some((c, w));
                        }
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Auto (from file)").clicked() {
                    if let Some(cell) = app.cells.get_mut(app.active_cell) {
                        cell.window = None;
                    }
                    ui.close_menu();
                }
            });

            // Right-aligned active-cell indicator.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!(
                    "Cell {}/{}",
                    app.active_cell + 1,
                    app.cells.len()
                ));
            });
        });
    });
}

fn tool_button(ui: &mut egui::Ui, active: &mut ActiveTool, this: ActiveTool, label: &str) {
    let selected = *active == this;
    let hint = match this {
        ActiveTool::WindowLevel => "Drag horizontal = width, vertical = center",
        ActiveTool::Pan => "Drag to pan (also: middle-mouse)",
        ActiveTool::Zoom => "Drag up/down to zoom (also: Ctrl/Cmd + scroll)",
        ActiveTool::Length => "Click 2 points (mm if pixel spacing present)",
        ActiveTool::Angle => "Click 3 points: side, vertex, side",
        ActiveTool::RectRoi => "Drag a rectangle (mean/std/min/max)",
        ActiveTool::EllipseRoi => "Drag an ellipse bbox (mean/std/min/max)",
    };
    if ui
        .selectable_label(selected, label)
        .on_hover_text(hint)
        .clicked()
    {
        *active = this;
    }
}

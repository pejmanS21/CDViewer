//! Grid viewport. Each cell is independently transformable
//! (pan/zoom/rotate/flip/invert/window) and is a drag-and-drop target for
//! series from the study browser. Measurement tools (length, angle,
//! rectangle ROI, ellipse ROI) draw into the active cell.

use crate::app::{CellState, DicomViewerApp};
use crate::dcm::annotation::Annotation;
use crate::dcm::roi::{angle_deg, ellipse_stats, length_label, rect_stats};
use crate::dcm::study::Instance;
use crate::dcm::{self, RawImage};
use crate::ui::theme;
use crate::ui::{study_browser::SeriesDragPayload, ActiveTool, InProgress};
use egui::epaint::{Mesh, Vertex};
use egui::{
    Align2, CentralPanel, Color32, ColorImage, Context, DragAndDrop, FontId, Painter, Pos2, Rect,
    Response, Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};
use std::sync::Arc;

const LEN_COLOR: Color32 = theme::ACCENT;
const ANG_COLOR: Color32 = Color32::from_rgb(0xE8, 0xC8, 0x6D);
const RECT_COLOR: Color32 = theme::CYAN_DATA;
const ELL_COLOR: Color32 = Color32::from_rgb(0x9C, 0xE0, 0xB4);
const PROG_COLOR: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x6C);

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    CentralPanel::default()
        .frame(
            egui::Frame::default()
                .fill(theme::BG)
                .inner_margin(egui::Margin::same(2.0)),
        )
        .show(ctx, |ui| {
            let avail = ui.available_rect_before_wrap();
            let (cols, rows) = app.grid.dims();
            let gap = 2.0;
            let cw = (avail.width() - gap * (cols as f32 - 1.0)) / cols as f32;
            let ch = (avail.height() - gap * (rows as f32 - 1.0)) / rows as f32;

            let n = app.grid.cell_count();
            for cell_idx in 0..n {
                let col = cell_idx % cols;
                let row = cell_idx / cols;
                let origin = Pos2::new(
                    avail.min.x + col as f32 * (cw + gap),
                    avail.min.y + row as f32 * (ch + gap),
                );
                let cell_rect = Rect::from_min_size(origin, Vec2::new(cw, ch));
                draw_cell(ui, ctx, app, cell_idx, cell_rect);
            }
            ui.allocate_rect(avail, Sense::hover());
        });
}

fn draw_cell(ui: &mut Ui, ctx: &Context, app: &mut DicomViewerApp, cell_idx: usize, rect: Rect) {
    let is_active = cell_idx == app.active_cell;
    paint_cell_contents(ui, ctx, app, cell_idx, rect, is_active);

    let (pointer_pos, pointer_released) =
        ctx.input(|i| (i.pointer.interact_pos(), i.pointer.any_released()));
    let hovered = pointer_pos.is_some_and(|p| rect.contains(p));

    if let Some(payload) = DragAndDrop::payload::<SeriesDragPayload>(ctx) {
        if hovered {
            // A glowing inner stroke + 4 amber brackets — instrumentation feel
            // rather than a generic yellow rectangle.
            let painter = ui.painter_at(rect);
            painter.rect_filled(
                rect.shrink(1.0),
                0.0,
                Color32::from_rgba_unmultiplied(0xE8, 0xA2, 0x2D, 22),
            );
            paint_brackets_four(&painter, rect.shrink(3.0), theme::ACCENT, 18.0, 1.5);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "  DROP TO LOAD  ",
                FontId::monospace(11.0),
                theme::ACCENT,
            );
        }
        if hovered && pointer_released {
            let series_ref = *payload;
            DragAndDrop::clear_payload(ctx);
            app.drop_series_onto_cell(cell_idx, series_ref);
        }
    }
}

fn paint_cell_contents(
    ui: &mut Ui,
    ctx: &Context,
    app: &mut DicomViewerApp,
    cell_idx: usize,
    rect: Rect,
    is_active: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(10));

    let Some((si, se)) = app.cells[cell_idx].series_ref else {
        draw_empty_cell(ui, rect, is_active);
        let id = ui.id().with(("cell-empty", cell_idx));
        let resp = ui.interact(rect, id, Sense::click());
        if resp.clicked() {
            app.active_cell = cell_idx;
        }
        return;
    };
    let Some(series) = app.studies.get(si).and_then(|s| s.series.get(se)).cloned() else {
        draw_empty_cell(ui, rect, is_active);
        return;
    };
    let count = series.instances.len();
    if count == 0 {
        draw_empty_cell(ui, rect, is_active);
        return;
    }
    if app.cells[cell_idx].slice >= count {
        app.cells[cell_idx].slice = count - 1;
    }
    let slice = app.cells[cell_idx].slice;
    let instance = series.instances[slice].clone();

    let Some(raw) = app.raw_for(&instance.sop_instance_uid, &instance.path) else {
        let msg = app
            .raw_failures
            .get(&instance.sop_instance_uid)
            .cloned()
            .unwrap_or_else(|| "Decoding…".to_string());
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::proportional(13.0),
            Color32::from_gray(170),
        );
        return;
    };

    let window = app.cells[cell_idx]
        .window
        .unwrap_or_else(|| dcm::auto_window(&raw));
    let user_invert = app.cells[cell_idx].invert;

    ensure_texture(
        ctx,
        app,
        cell_idx,
        &instance.sop_instance_uid,
        &raw,
        window,
        user_invert,
    );

    let dst = compute_dst_rect(
        raw.width as f32,
        raw.height as f32,
        &app.cells[cell_idx],
        rect,
    );
    handle_input(
        ui, app, cell_idx, count, &raw, rect, dst, &instance, is_active,
    );

    let cell = &app.cells[cell_idx];
    let Some(tex) = cell.tex.clone() else {
        return;
    };

    paint_image(ui.painter_at(rect), &tex, rect, dst, cell);

    if app.ui_state.annotations_visible {
        let anns = app
            .annotation_store
            .for_instance(&instance.sop_instance_uid);
        let painter = ui.painter_at(rect);
        for ann in anns {
            draw_annotation(&painter, ann, &raw, &instance, &series.modality, cell, dst);
        }
    }

    if is_active {
        if let Some(prog) = app.ui_state.in_progress {
            draw_in_progress(&ui.painter_at(rect), prog, &raw, cell, dst);
        }
    }

    paint_corner_overlays(
        &ui.painter_at(rect),
        rect,
        cell_idx,
        app.cells.len(),
        &series.modality,
        &instance,
        slice,
        count,
        window,
        cell,
        user_invert,
    );

    // Active-cell indicator: amber viewfinder brackets, not a solid rect.
    if is_active {
        theme::paint_active_brackets(&ui.painter_at(rect), rect, theme::ACCENT, 14.0);
    } else {
        ui.painter()
            .rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(1.0, theme::LINE));
    }
}

fn draw_empty_cell(ui: &Ui, rect: Rect, is_active: bool) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::BG_DEEP);

    // Faint corner brackets even on empty cells — establishes that every
    // cell is a slot, not just an empty rectangle.
    paint_brackets_four(&painter, rect.shrink(6.0), theme::LINE_BRIGHT, 10.0, 1.0);

    // A subtle centred reticle.
    let c = rect.center();
    let s = Stroke::new(1.0, theme::LINE_BRIGHT);
    let arm = 8.0;
    let gap = 3.0;
    painter.line_segment([c - Vec2::new(arm, 0.0), c - Vec2::new(gap, 0.0)], s);
    painter.line_segment([c + Vec2::new(gap, 0.0), c + Vec2::new(arm, 0.0)], s);
    painter.line_segment([c - Vec2::new(0.0, arm), c - Vec2::new(0.0, gap)], s);
    painter.line_segment([c + Vec2::new(0.0, gap), c + Vec2::new(0.0, arm)], s);
    painter.circle_stroke(c, 1.5, s);

    painter.text(
        c + Vec2::new(0.0, 22.0),
        Align2::CENTER_CENTER,
        "—  drag a series  —",
        FontId::proportional(11.0),
        theme::FG_FAINT,
    );

    if is_active {
        theme::paint_active_brackets(&painter, rect, theme::ACCENT, 14.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_corner_overlays(
    painter: &Painter,
    rect: Rect,
    cell_idx: usize,
    cell_total: usize,
    modality: &str,
    instance: &Instance,
    slice: usize,
    count: usize,
    window: (f64, f64),
    cell: &CellState,
    user_invert: bool,
) {
    let pad = Vec2::new(8.0, 6.0);
    let mono = FontId::monospace(11.0);
    let mono_big = FontId::monospace(14.0);
    let prop = FontId::proportional(10.5);

    // Top-left: modality chip + view label
    let chip = format!(" {} ", modality);
    let chip_color = theme::modality_color(modality);
    let chip_galley = painter.layout_no_wrap(chip.clone(), mono.clone(), theme::BG);
    let chip_rect =
        Rect::from_min_size(rect.left_top() + pad, chip_galley.size()).expand2(Vec2::new(2.0, 1.0));
    painter.rect_filled(chip_rect, 0.0, chip_color);
    painter.galley(
        chip_rect.left_top() + Vec2::new(2.0, 1.0),
        chip_galley,
        theme::BG,
    );

    // View hint (MG laterality/view position, or "AX/SAG/COR" placeholder)
    if let Some(lat) = instance.image_laterality.as_deref() {
        let view = instance.view_position.as_deref().unwrap_or("");
        let tag = format!("{lat} · {view}")
            .trim_end_matches(" · ")
            .to_string();
        painter.text(
            chip_rect.right_top() + Vec2::new(6.0, 0.0),
            Align2::LEFT_TOP,
            tag,
            mono.clone(),
            theme::FG,
        );
    }

    // Top-right: cell number / total
    let cell_label = format!("CELL {:02}/{:02}", cell_idx + 1, cell_total);
    painter.text(
        rect.right_top() + Vec2::new(-pad.x, pad.y),
        Align2::RIGHT_TOP,
        cell_label,
        mono.clone(),
        theme::FG_DIM,
    );

    // Bottom-left: slice indicator (big number, tabular)
    let slice_big = format!("{:03}", slice + 1);
    let slice_dim = format!(" / {:03}", count);
    let g_big = painter.layout_no_wrap(slice_big.clone(), mono_big.clone(), theme::ACCENT);
    let bl = rect.left_bottom() + Vec2::new(pad.x, -pad.y - g_big.size().y);
    painter.galley(bl, g_big.clone(), theme::ACCENT);
    painter.text(
        bl + Vec2::new(g_big.size().x, g_big.size().y * 0.55),
        Align2::LEFT_BOTTOM,
        &slice_dim,
        mono.clone(),
        theme::FG_DIM,
    );
    painter.text(
        bl + Vec2::new(0.0, -1.0),
        Align2::LEFT_BOTTOM,
        "SLICE",
        prop.clone(),
        theme::FG_FAINT,
    );

    // Bottom-right: window/level + zoom + transform flags, stacked
    let mut flags = String::new();
    if user_invert {
        flags.push_str(" INV");
    }
    if cell.flip_h {
        flags.push_str(" ⇄H");
    }
    if cell.flip_v {
        flags.push_str(" ⇅V");
    }
    match cell.rotation_quarter % 4 {
        1 => flags.push_str(" ⟳90"),
        2 => flags.push_str(" ⟳180"),
        3 => flags.push_str(" ⟳270"),
        _ => {}
    }
    let line_wl = format!("C {:>5.0}   W {:>5.0}", window.0, window.1);
    let line_z = format!("ZOOM {:>5.0}%", cell.zoom * 100.0);

    let br = rect.right_bottom() + Vec2::new(-pad.x, -pad.y);
    let g_wl = painter.layout_no_wrap(line_wl, mono.clone(), theme::CYAN_DATA);
    let g_z = painter.layout_no_wrap(line_z, mono.clone(), theme::FG_DIM);
    let g_f = painter.layout_no_wrap(flags.trim().to_string(), mono.clone(), theme::ACCENT);

    let mut y = br.y;
    if !g_f.is_empty() {
        y -= g_f.size().y;
        painter.galley(Pos2::new(br.x - g_f.size().x, y), g_f, theme::ACCENT);
    }
    y -= g_z.size().y + 1.0;
    painter.galley(Pos2::new(br.x - g_z.size().x, y), g_z, theme::FG_DIM);
    y -= g_wl.size().y + 1.0;
    painter.galley(Pos2::new(br.x - g_wl.size().x, y), g_wl, theme::CYAN_DATA);

    // Patient name (subtle, top-centre) — small reminder of context
    if !instance.modality.is_empty() {
        // Already shown via chip; keep patient name only if non-empty in instance.
    }
}

fn paint_brackets_four(painter: &Painter, rect: Rect, color: Color32, arm: f32, width: f32) {
    let s = Stroke::new(width, color);
    let tl = rect.left_top();
    let tr = rect.right_top();
    let bl = rect.left_bottom();
    let br = rect.right_bottom();
    painter.line_segment([tl, Pos2::new(tl.x + arm, tl.y)], s);
    painter.line_segment([tl, Pos2::new(tl.x, tl.y + arm)], s);
    painter.line_segment([tr, Pos2::new(tr.x - arm, tr.y)], s);
    painter.line_segment([tr, Pos2::new(tr.x, tr.y + arm)], s);
    painter.line_segment([bl, Pos2::new(bl.x + arm, bl.y)], s);
    painter.line_segment([bl, Pos2::new(bl.x, bl.y - arm)], s);
    painter.line_segment([br, Pos2::new(br.x - arm, br.y)], s);
    painter.line_segment([br, Pos2::new(br.x, br.y - arm)], s);
}

fn ensure_texture(
    ctx: &Context,
    app: &mut DicomViewerApp,
    cell_idx: usize,
    sop_uid: &str,
    raw: &Arc<RawImage>,
    window: (f64, f64),
    invert: bool,
) {
    let need = {
        let c = &app.cells[cell_idx];
        c.tex.is_none()
            || c.tex_uid.as_deref() != Some(sop_uid)
            || c.tex_window != Some(window)
            || c.tex_invert != invert
    };
    if !need {
        return;
    }
    // Defensive: only upload textures with a consistent size. Bail
    // loudly into the log instead of panicking inside ColorImage.
    let expected = (raw.width as usize).saturating_mul(raw.height as usize);
    if raw.width == 0 || raw.height == 0 || expected == 0 {
        tracing::warn!(
            uid = sop_uid,
            w = raw.width,
            h = raw.height,
            "skip texture: empty image"
        );
        return;
    }
    let rgba = dcm::render_rgba(raw, window.0, window.1, invert);
    if rgba.len() != expected * 4 {
        tracing::warn!(
            uid = sop_uid,
            got = rgba.len(),
            expected = expected * 4,
            "skip texture: rgba size mismatch"
        );
        return;
    }
    let img = ColorImage::from_rgba_unmultiplied([raw.width as usize, raw.height as usize], &rgba);
    let tex = ctx.load_texture(
        format!("cell-{cell_idx}-{sop_uid}"),
        img,
        TextureOptions::LINEAR,
    );
    let c = &mut app.cells[cell_idx];
    c.tex = Some(tex);
    c.tex_uid = Some(sop_uid.to_string());
    c.tex_window = Some(window);
    c.tex_invert = invert;
}

fn compute_dst_rect(img_w: f32, img_h: f32, cell: &CellState, rect: Rect) -> Rect {
    use crate::app::{FitMode, HAnchor};
    let (eff_w, eff_h) = if cell.rotation_quarter % 2 == 1 {
        (img_h, img_w)
    } else {
        (img_w, img_h)
    };
    let scale = match cell.fit_mode {
        FitMode::Contain => (rect.width() / eff_w.max(1.0)).min(rect.height() / eff_h.max(1.0)),
        FitMode::Cover => (rect.width() / eff_w.max(1.0)).max(rect.height() / eff_h.max(1.0)),
    };
    let z = scale * cell.zoom;
    let dst_size = Vec2::new(eff_w * z, eff_h * z);
    // Anchor establishes the base x; user pan offsets from there. y is
    // always centred (MG hanging only anchors horizontally).
    let base_x = match cell.h_anchor {
        HAnchor::Center => rect.center().x,
        HAnchor::Left => rect.left() + dst_size.x * 0.5,
        HAnchor::Right => rect.right() - dst_size.x * 0.5,
    };
    let center = egui::pos2(base_x + cell.pan.x, rect.center().y + cell.pan.y);
    Rect::from_center_size(center, dst_size)
}

/// Map image-pixel coords -> screen Pos2 honouring flips + rotation + pan + zoom.
fn image_to_screen(img_xy: [f32; 2], raw_w: f32, raw_h: f32, cell: &CellState, dst: Rect) -> Pos2 {
    let mut u = (img_xy[0] / raw_w.max(1.0)).clamp(-10.0, 10.0);
    let mut v = (img_xy[1] / raw_h.max(1.0)).clamp(-10.0, 10.0);
    if cell.flip_h {
        u = 1.0 - u;
    }
    if cell.flip_v {
        v = 1.0 - v;
    }
    let (uu, vv) = match cell.rotation_quarter % 4 {
        0 => (u, v),
        1 => (1.0 - v, u),
        2 => (1.0 - u, 1.0 - v),
        3 => (v, 1.0 - u),
        _ => (u, v),
    };
    dst.min + Vec2::new(uu * dst.width(), vv * dst.height())
}

fn screen_to_image(screen: Pos2, raw_w: f32, raw_h: f32, cell: &CellState, dst: Rect) -> [f32; 2] {
    let uu = (screen.x - dst.min.x) / dst.width().max(1e-3);
    let vv = (screen.y - dst.min.y) / dst.height().max(1e-3);
    let (u, v) = match cell.rotation_quarter % 4 {
        0 => (uu, vv),
        1 => (vv, 1.0 - uu),
        2 => (1.0 - uu, 1.0 - vv),
        3 => (1.0 - vv, uu),
        _ => (uu, vv),
    };
    let mut u = u;
    let mut v = v;
    if cell.flip_h {
        u = 1.0 - u;
    }
    if cell.flip_v {
        v = 1.0 - v;
    }
    [u * raw_w, v * raw_h]
}

#[allow(clippy::too_many_arguments)]
fn handle_input(
    ui: &mut Ui,
    app: &mut DicomViewerApp,
    cell_idx: usize,
    slice_count: usize,
    raw: &Arc<RawImage>,
    rect: Rect,
    dst: Rect,
    instance: &Instance,
    is_active: bool,
) {
    let id = ui.id().with(("cell", cell_idx));
    let resp = ui.interact(rect, id, Sense::click_and_drag());

    if resp.clicked() || resp.drag_started() {
        app.active_cell = cell_idx;
    }

    let hovered = resp.hovered();
    let (modifiers, scroll, middle_down) = ui.input(|i| {
        (
            i.modifiers,
            i.smooth_scroll_delta.y + i.raw_scroll_delta.y,
            i.pointer.middle_down(),
        )
    });

    // Scroll: slice change (plain) / zoom (ctrl/cmd). Active for every tool.
    if hovered {
        if (modifiers.ctrl || modifiers.command) && scroll.abs() > 0.5 {
            let f = (scroll * 0.005).exp();
            let c = &mut app.cells[cell_idx];
            c.zoom = (c.zoom * f).clamp(0.05, 50.0);
        } else if scroll.abs() > 0.5 && slice_count > 1 {
            let step: i64 = if scroll > 0.0 { -1 } else { 1 };
            let c = &mut app.cells[cell_idx];
            c.slice = (c.slice as i64 + step).clamp(0, slice_count as i64 - 1) as usize;
        }
    }

    let tool = app.ui_state.active_tool;
    let raw_w = raw.width as f32;
    let raw_h = raw.height as f32;
    let uid = instance.sop_instance_uid.clone();

    if tool.is_measurement() {
        handle_measurement(
            app,
            cell_idx,
            &resp,
            raw_w,
            raw_h,
            dst,
            &uid,
            tool,
            middle_down,
        );
    } else if resp.dragged() {
        let drag = resp.drag_delta();
        let pan_mode = middle_down || tool == ActiveTool::Pan;
        let c = &mut app.cells[cell_idx];
        if pan_mode {
            c.pan += drag;
        } else {
            match tool {
                ActiveTool::Zoom => {
                    let f = (-drag.y * 0.01).exp();
                    c.zoom = (c.zoom * f).clamp(0.05, 50.0);
                }
                ActiveTool::WindowLevel => {
                    let range = (raw.max_val - raw.min_val).max(1.0) as f64;
                    let sens = range / 400.0;
                    let current = c.window.unwrap_or_else(|| dcm::auto_window(raw));
                    let new_w = (current.1 + drag.x as f64 * sens).max(1.0);
                    let new_c = current.0 + drag.y as f64 * sens;
                    c.window = Some((new_c, new_w));
                }
                _ => {}
            }
        }
    }

    if is_active && hovered {
        ui.input(|i| {
            let c = &mut app.cells[cell_idx];
            if i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::ArrowRight) {
                c.slice = (c.slice + 1).min(slice_count.saturating_sub(1));
            }
            if i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::ArrowLeft) {
                c.slice = c.slice.saturating_sub(1);
            }
            if i.key_pressed(egui::Key::Home) {
                c.slice = 0;
            }
            if i.key_pressed(egui::Key::End) {
                c.slice = slice_count.saturating_sub(1);
            }
            if i.key_pressed(egui::Key::Escape) {
                app.ui_state.in_progress = None;
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_measurement(
    app: &mut DicomViewerApp,
    cell_idx: usize,
    resp: &Response,
    raw_w: f32,
    raw_h: f32,
    dst: Rect,
    uid: &str,
    tool: ActiveTool,
    middle_down: bool,
) {
    // Middle-mouse pan still works in measurement mode.
    if middle_down && resp.dragged() {
        let drag = resp.drag_delta();
        let c = &mut app.cells[cell_idx];
        c.pan += drag;
        return;
    }

    let cell = app.cells[cell_idx].clone();

    match tool {
        ActiveTool::Length | ActiveTool::Angle => {
            if let (true, Some(pos)) = (resp.clicked(), resp.interact_pointer_pos()) {
                let img_pt = screen_to_image(pos, raw_w, raw_h, &cell, dst);
                match tool {
                    ActiveTool::Length => match app.ui_state.in_progress {
                        Some(InProgress::LengthP1(p1)) => {
                            app.push_annotation(uid, Annotation::Length { p1, p2: img_pt });
                            app.ui_state.in_progress = None;
                        }
                        _ => {
                            app.ui_state.in_progress = Some(InProgress::LengthP1(img_pt));
                        }
                    },
                    ActiveTool::Angle => match app.ui_state.in_progress {
                        None => {
                            app.ui_state.in_progress = Some(InProgress::AngleP1(img_pt));
                        }
                        Some(InProgress::AngleP1(p1)) => {
                            app.ui_state.in_progress = Some(InProgress::AngleP1V(p1, img_pt));
                        }
                        Some(InProgress::AngleP1V(p1, v)) => {
                            app.push_annotation(uid, Annotation::Angle { p1, v, p2: img_pt });
                            app.ui_state.in_progress = None;
                        }
                        _ => {
                            app.ui_state.in_progress = Some(InProgress::AngleP1(img_pt));
                        }
                    },
                    _ => {}
                }
            }
        }
        ActiveTool::RectRoi | ActiveTool::EllipseRoi => {
            if resp.drag_started() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let pt = screen_to_image(pos, raw_w, raw_h, &cell, dst);
                    app.ui_state.in_progress = Some(match tool {
                        ActiveTool::RectRoi => InProgress::RectDrag { start: pt, cur: pt },
                        ActiveTool::EllipseRoi => InProgress::EllipseDrag { start: pt, cur: pt },
                        _ => unreachable!(),
                    });
                }
            }
            if resp.dragged() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let pt = screen_to_image(pos, raw_w, raw_h, &cell, dst);
                    if let Some(ref mut prog) = app.ui_state.in_progress {
                        match prog {
                            InProgress::RectDrag { cur, .. } => *cur = pt,
                            InProgress::EllipseDrag { cur, .. } => *cur = pt,
                            _ => {}
                        }
                    }
                }
            }
            if resp.drag_stopped() {
                if let Some(prog) = app.ui_state.in_progress.take() {
                    // Drop sub-pixel "rectangles" — they're a single click,
                    // not an intentional ROI. Match guards keep clippy
                    // `collapsible_match` happy on Rust 1.95+.
                    let is_significant = |start: [f32; 2], cur: [f32; 2]| {
                        (cur[0] - start[0]).abs() > 2.0 || (cur[1] - start[1]).abs() > 2.0
                    };
                    match prog {
                        InProgress::RectDrag { start, cur } if is_significant(start, cur) => {
                            app.push_annotation(uid, Annotation::Rect { p1: start, p2: cur });
                        }
                        InProgress::EllipseDrag { start, cur } if is_significant(start, cur) => {
                            app.push_annotation(uid, Annotation::Ellipse { p1: start, p2: cur });
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn paint_image(painter: Painter, tex: &TextureHandle, rect: Rect, dst: Rect, cell: &CellState) {
    let mut uvs = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    if cell.flip_h {
        uvs.swap(0, 1);
        uvs.swap(3, 2);
    }
    if cell.flip_v {
        uvs.swap(0, 3);
        uvs.swap(1, 2);
    }
    let rotated_uvs = match cell.rotation_quarter % 4 {
        0 => uvs,
        1 => [uvs[3], uvs[0], uvs[1], uvs[2]],
        2 => [uvs[2], uvs[3], uvs[0], uvs[1]],
        3 => [uvs[1], uvs[2], uvs[3], uvs[0]],
        _ => uvs,
    };
    let positions = [
        dst.left_top(),
        dst.right_top(),
        dst.right_bottom(),
        dst.left_bottom(),
    ];
    let mut mesh = Mesh::with_texture(tex.id());
    for i in 0..4 {
        mesh.vertices.push(Vertex {
            pos: positions[i],
            uv: rotated_uvs[i],
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    let clipped = painter.with_clip_rect(rect);
    clipped.add(Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
fn draw_annotation(
    painter: &Painter,
    ann: &Annotation,
    raw: &RawImage,
    inst: &Instance,
    modality: &str,
    cell: &CellState,
    dst: Rect,
) {
    let raw_w = raw.width as f32;
    let raw_h = raw.height as f32;
    let to_screen = |p: [f32; 2]| -> Pos2 { image_to_screen(p, raw_w, raw_h, cell, dst) };
    match ann {
        Annotation::Length { p1, p2 } => {
            let a = to_screen(*p1);
            let b = to_screen(*p2);
            let stroke = Stroke::new(1.5, LEN_COLOR);
            painter.line_segment([a, b], stroke);
            painter.circle_filled(a, 3.0, LEN_COLOR);
            painter.circle_filled(b, 3.0, LEN_COLOR);
            let mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let label = length_label(*p1, *p2, inst, raw);
            label_text(painter, mid, &label, LEN_COLOR);
        }
        Annotation::Angle { p1, v, p2 } => {
            let s_p1 = to_screen(*p1);
            let s_v = to_screen(*v);
            let s_p2 = to_screen(*p2);
            let stroke = Stroke::new(1.5, ANG_COLOR);
            painter.line_segment([s_v, s_p1], stroke);
            painter.line_segment([s_v, s_p2], stroke);
            painter.circle_filled(s_p1, 3.0, ANG_COLOR);
            painter.circle_filled(s_v, 3.0, ANG_COLOR);
            painter.circle_filled(s_p2, 3.0, ANG_COLOR);
            let deg = angle_deg(*p1, *v, *p2);
            label_text(
                painter,
                s_v + Vec2::new(8.0, -14.0),
                &format!("{deg:.1}°"),
                ANG_COLOR,
            );
        }
        Annotation::Rect { p1, p2 } => {
            let a = to_screen(*p1);
            let b = to_screen(*p2);
            let r = Rect::from_two_pos(a, b);
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, RECT_COLOR));
            if let Some(stats) = rect_stats(raw, *p1, *p2) {
                label_text(
                    painter,
                    r.left_top() - Vec2::new(0.0, 14.0),
                    &stats.label(modality),
                    RECT_COLOR,
                );
            }
        }
        Annotation::Ellipse { p1, p2 } => {
            let a = to_screen(*p1);
            let b = to_screen(*p2);
            let r = Rect::from_two_pos(a, b);
            draw_ellipse(painter, r, ELL_COLOR, 1.5);
            if let Some(stats) = ellipse_stats(raw, *p1, *p2) {
                label_text(
                    painter,
                    r.left_top() - Vec2::new(0.0, 14.0),
                    &stats.label(modality),
                    ELL_COLOR,
                );
            }
        }
    }
}

fn draw_in_progress(
    painter: &Painter,
    prog: InProgress,
    raw: &RawImage,
    cell: &CellState,
    dst: Rect,
) {
    let raw_w = raw.width as f32;
    let raw_h = raw.height as f32;
    let to_screen = |p: [f32; 2]| image_to_screen(p, raw_w, raw_h, cell, dst);
    let stroke = Stroke::new(1.5, PROG_COLOR);
    match prog {
        InProgress::LengthP1(p) | InProgress::AngleP1(p) => {
            painter.circle_filled(to_screen(p), 4.0, PROG_COLOR);
        }
        InProgress::AngleP1V(p1, v) => {
            painter.line_segment([to_screen(v), to_screen(p1)], stroke);
            painter.circle_filled(to_screen(p1), 4.0, PROG_COLOR);
            painter.circle_filled(to_screen(v), 4.0, PROG_COLOR);
        }
        InProgress::RectDrag { start, cur } => {
            let r = Rect::from_two_pos(to_screen(start), to_screen(cur));
            painter.rect_stroke(r, 0.0, stroke);
        }
        InProgress::EllipseDrag { start, cur } => {
            let r = Rect::from_two_pos(to_screen(start), to_screen(cur));
            draw_ellipse(painter, r, PROG_COLOR, 1.5);
        }
    }
}

fn draw_ellipse(painter: &Painter, r: Rect, color: Color32, width: f32) {
    let cx = r.center().x;
    let cy = r.center().y;
    let rx = r.width().abs() / 2.0;
    let ry = r.height().abs() / 2.0;
    let steps = 64;
    let mut pts: Vec<Pos2> = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = (i as f32) * std::f32::consts::TAU / steps as f32;
        pts.push(Pos2::new(cx + rx * t.cos(), cy + ry * t.sin()));
    }
    painter.add(Shape::line(pts, Stroke::new(width, color)));
}

fn label_text(painter: &Painter, pos: Pos2, text: &str, color: Color32) {
    // Drop a translucent background so text stays readable on busy images.
    let font = FontId::monospace(11.0);
    let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    let bg_rect = Rect::from_min_size(pos, galley.size()).expand(2.0);
    painter.rect_filled(bg_rect, 2.0, Color32::from_black_alpha(160));
    painter.galley(pos, galley, color);
}

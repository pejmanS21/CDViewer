use crate::app::DicomViewerApp;
use egui::{Context, Window};

pub fn draw(ctx: &Context, app: &mut DicomViewerApp) {
    if !app.ui_state.show_about {
        return;
    }
    let mut open = true;
    Window::new("About DICOM Viewer")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("DICOM Viewer v{}", env!("CARGO_PKG_VERSION")));
            ui.add_space(4.0);
            ui.weak("Portable, non-diagnostic DICOM viewer.");
            ui.add_space(8.0);
            ui.label(format!(
                "Exe dir:  {}\nData dir: {}",
                app.paths.exe_dir.display(),
                app.paths.data_dir.display()
            ));
        });
    if !open {
        app.ui_state.show_about = false;
    }
}

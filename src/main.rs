#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use anyhow::Result;
use dicom_viewer::app::DicomViewerApp;
use dicom_viewer::{config, logging};
use tracing::info;

fn main() -> Result<()> {
    let paths = config::Paths::resolve()?;
    let _log_guard = logging::init(&paths)?;

    // Log panics to file before the process aborts (release builds use
    // panic=abort so otherwise crashes vanish silently).
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<no message>".to_string());
        tracing::error!(target: "panic", location = %loc, "PANIC: {msg}");
        let bt = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "panic", "{bt}");
        prev(info);
    }));

    info!(
        version = env!("CARGO_PKG_VERSION"),
        portable = paths.is_portable(),
        exe_dir = %paths.exe_dir.display(),
        data_dir = %paths.data_dir.display(),
        "starting dicom-viewer"
    );

    let cfg = config::Config::load(&paths).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "config load failed, using defaults");
        config::Config::default()
    });

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DICOM Viewer")
            .with_inner_size([cfg.window.width, cfg.window.height])
            .with_min_inner_size([720.0, 480.0]),
        vsync: true,
        ..Default::default()
    };

    let paths_for_app = paths.clone();
    let cfg_for_app = cfg.clone();

    eframe::run_native(
        "dicom-viewer",
        native_options,
        Box::new(move |cc| Ok(Box::new(DicomViewerApp::new(cc, paths_for_app, cfg_for_app)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

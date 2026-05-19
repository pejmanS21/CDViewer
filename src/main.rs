#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use dicom_viewer::app::DicomViewerApp;
use dicom_viewer::{config, logging};
use std::path::PathBuf;
use tracing::info;

/// Folder names probed next to the executable when no CLI argument is
/// passed. First match wins. These are the conventional CD-burn names.
const AUTOLOAD_DIRS: &[&str] = &["DICOM", "dicom", "IMAGES", "images", "DICOMDIR"];

/// CLI: `dicom-viewer [PATH]`. If `PATH` is given and exists, that's the
/// startup folder. Otherwise we look for a sibling `DICOM/` on the same
/// volume as the binary — the standard layout for autorun CDs.
fn detect_startup_folder(paths: &config::Paths) -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        let p = PathBuf::from(arg);
        if p.exists() {
            info!(path = %p.display(), "startup folder from CLI arg");
            return Some(p);
        } else {
            tracing::warn!(arg = %p.display(), "CLI arg path does not exist");
        }
    }
    if let Ok(env) = std::env::var("DICOM_VIEWER_DATA") {
        let p = PathBuf::from(env);
        if p.exists() {
            info!(path = %p.display(), "startup folder from env");
            return Some(p);
        }
    }
    for name in AUTOLOAD_DIRS {
        let p = paths.exe_dir.join(name);
        if p.is_dir() {
            info!(path = %p.display(), "startup folder auto-detected next to exe");
            return Some(p);
        }
    }
    None
}

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
    let startup_folder = detect_startup_folder(&paths);

    eframe::run_native(
        "dicom-viewer",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(DicomViewerApp::new(
                cc,
                paths_for_app,
                cfg_for_app,
                startup_folder,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

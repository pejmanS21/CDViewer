//! `tracing` setup writing a daily-rolled log to `logs/` next to the binary.
//!
//! Two layers are installed:
//!
//! * a non-blocking file writer producing `dicom-viewer.log.<YYYY-MM-DD>`
//!   files (no ANSI colour, with target prefix);
//! * the standard `tracing-subscriber` `fmt::layer` for stderr (so
//!   `cargo run` shows the logs).
//!
//! The filter respects `RUST_LOG`, defaulting to
//! `info,wgpu_core=warn,wgpu_hal=warn,naga=warn` when unset.

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Paths;

/// Initialise global tracing.
///
/// Returns a [`WorkerGuard`]; the caller **must keep it alive** for the
/// process lifetime, otherwise buffered log lines may be dropped on exit.
///
/// # Errors
/// Filesystem failures setting up the rolling file appender.
pub fn init(paths: &Paths) -> Result<WorkerGuard> {
    let file_appender = tracing_appender::rolling::daily(&paths.logs_dir, "dicom-viewer.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"));

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true);

    let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_target(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    Ok(guard)
}

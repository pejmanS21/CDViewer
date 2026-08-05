//! # CDViewer — a portable, non-diagnostic DICOM viewer
//!
//! `dicom-viewer` is a single-executable, statically linked DICOM viewer in
//! Rust, primarily targeting Windows but with first-class Linux and macOS
//! support. It is designed to ship on study CDs/DVDs alongside the imaging
//! data and run without installation.
//!
//! > **Not for diagnostic use.** This viewer is intended for review and
//! > reference only. A "Not for diagnostic use" disclaimer is shown on first
//! > launch and acknowledged in `config.toml`.
//!
//! ## What lives where
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`app`] | The single [`eframe::App`] — owns viewer state, drives the UI |
//! | [`config`] | Portable-first path resolution + `config.toml` load/save |
//! | [`logging`] | `tracing` setup writing a daily-rolled log under `<data_dir>/logs/` |
//! | [`dcm`] | DICOM domain — folder scan, pixel decode, ROI stats, annotations, export, metadata |
//! | [`ui`] | egui composition — menu, toolbar, study browser, viewport, dialogs, status bar |
//!
//! `src/main.rs` is intentionally thin: it resolves paths, sets up logging,
//! installs the panic hook, and hands off to [`eframe::run_native`].
//! Anything that needs to be tested lives in the library and is exposed
//! through this crate root so [`tests/sample_data.rs`][tests] can call it.
//!
//! [tests]: https://github.com/pejmanS21/CDViewer/blob/main/tests/sample_data.rs
//!
//! ## DICOM pipeline overview
//!
//! 1. [`dcm::loader::load_folder`] walks the directory in parallel
//!    ([`rayon`](https://docs.rs/rayon)), recognises files by extension or
//!    by probing the `DICM` magic at byte 128, and reads metadata with
//!    `OpenFileOptions::read_until(PIXEL_DATA)`. Pixel data is **never**
//!    decoded during the scan.
//! 2. The result is a `Vec<`[`dcm::Study`]`>` — purely metadata.
//! 3. When the viewport needs to draw a slice it calls
//!    [`dcm::pixel::load_raw`], which decodes the pixel data **once** into a
//!    rescaled `f32` buffer (modality LUT applied, VOI not applied) and
//!    caches the result in [`app::DicomViewerApp::raw_cache`] keyed by SOP
//!    Instance UID.
//! 4. [`dcm::pixel::render_rgba`] turns those `f32` values into RGBA8 with
//!    a window/level applied. It's cheap enough to call on every drag delta
//!    for interactive W/L.
//! 5. The egui [`TextureHandle`](egui::TextureHandle) is cached on the
//!    [`app::CellState`] and invalidated when the SOP UID, window, or invert
//!    setting drifts (see `ensure_texture` in `ui::viewport`).
//!
//! Measurement and ROI math lives in [`dcm::roi`]. Annotations are
//! persisted to a JSON sidecar by [`dcm::annotation::AnnotationStore`].
//! **Original DICOM files are never modified.**
//!
//! ## Locked-in design decisions
//!
//! These were chosen deliberately; don't change them without discussion:
//!
//! - **UI framework**: [`egui`] via [`eframe`] with the `glow` (OpenGL 3.3)
//!   renderer. `wgpu` is intentionally not enabled: it needs DX12
//!   (Windows 10 + feature-level 11 hardware) or Vulkan drivers, so it
//!   fails to *launch* on the older/weaker PCs a study CD lands on.
//!   OpenGL 3.3 reaches back to ~2011 Intel HD 3000, and dropping wgpu
//!   also drops `naga` and the DX12/Vulkan backends from the binary.
//! - **eframe `persistence` stays off**: window state is saved by
//!   [`config::Config`], annotations by [`dcm::annotation`].
//! - **DICOM**: the [`dicom-rs`](https://docs.rs/dicom) 0.7 family
//!   (`dicom`, `dicom-pixeldata`, `dicom-dictionary-std`).
//! - **Concurrency**: decode is on the UI thread. No `tokio`. MG-sized
//!   images are downsampled at load time (see
//!   [`dcm::pixel::RawImage::display_scale`]).
//! - **Release profile**: `lto = "fat"`, `codegen-units = 1`,
//!   `strip = true`, `opt-level = "z"`, `panic = "abort"`. The ~30 MB
//!   binary budget is real.
//! - **`image` crate features**: PNG only.
//!
//! ## Easy ways to break things
//!
//! - **Texture cache invalidation.** Any new visual modifier (e.g. gamma)
//!   must participate in the `(tex_uid, tex_window, tex_invert)` comparison
//!   in `ui::viewport::ensure_texture`.
//! - **Measurement coordinate space.** Annotations are stored in
//!   *displayed-image-pixel* coordinates (post-`display_scale`). Real-world
//!   units multiply by `pixel_spacing × display_scale`.
//! - **Drag-and-drop in egui 0.29.** Read [`ui`] for the specifics — do not
//!   wrap viewport cells in `dnd_drop_zone`.
//!
//! ## Further reading
//!
//! - [`README.md`](https://github.com/pejmanS21/CDViewer/blob/main/README.md)
//!   — install, usage, feature list.
//! - [`docs/ARCHITECTURE.md`](https://github.com/pejmanS21/CDViewer/blob/main/docs/ARCHITECTURE.md)
//!   — extended tour of the codebase.
//! - [`docs/CI-CD.md`](https://github.com/pejmanS21/CDViewer/blob/main/docs/CI-CD.md)
//!   — how CI, releases, and security scanning are wired up.
//! - [`CONTRIBUTING.md`](https://github.com/pejmanS21/CDViewer/blob/main/CONTRIBUTING.md)
//!   — local dev workflow.

#![doc(html_root_url = "https://docs.rs/dicom-viewer/0.1.0")]

pub mod app;
pub mod config;
pub mod dcm;
pub mod logging;
pub mod ui;

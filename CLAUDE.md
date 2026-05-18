# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A portable, single-executable, **non-diagnostic** DICOM viewer in Rust, primarily targeting Windows (Linux/macOS work too — the only OS-specific code path is the not-yet-implemented optical-media detection/burning). The viewer is intended for review and reference; a "Not for diagnostic use" disclaimer fires on first run and is persisted in `config.toml`.

## Common commands

```bash
# Dev build + run (window opens; logs to <exe_dir>/logs/dicom-viewer.log.<date>)
cargo run

# Release build — what users get. Targets <30 MB stripped.
cargo build --release
./target/release/dicom-viewer

# Lint — must stay clean. -D warnings is treated as CI gate.
cargo clippy --all-targets --all-features -- -D warnings

# All tests (integration tests live in tests/sample_data.rs and exercise
# real DICOM samples in sample-data/. Tests auto-skip if sample-data/ is
# absent so they pass on CI without fixtures.)
cargo test

# Single test by name (substring match):
cargo test rect_roi_stats_on_ct

# Tracing filter at runtime
RUST_LOG=dicom_viewer=debug,wgpu_core=warn cargo run
```

`sample-data/ct-data/` (30 CT slices) and `sample-data/mg-data/` (4 MG views) are the canonical fixtures used by the integration tests and by manual smoke tests.

## High-level architecture

### Library + binary split

`src/lib.rs` exposes every module (`app`, `config`, `dcm`, `logging`, `ui`); `src/main.rs` is a thin wrapper that only handles startup wiring (paths, logging, panic hook, eframe boot). **Integration tests go through the library** — they cannot use anything that's only in the binary. Keep new top-level modules behind `pub mod` in `lib.rs`.

### State lives in `DicomViewerApp` (`src/app.rs`)

One `eframe::App`, single-threaded UI. All state is owned here, including:

- `studies: Vec<Study>` — the loaded metadata tree (Study → Series → Instance). Pixels are **not** in here.
- `raw_cache: HashMap<sop_uid, Arc<RawImage>>` — pixel data decoded on demand by `dcm::load_raw`, keyed by SOP Instance UID. Survives slice/cell changes; cleared on `open_folder`.
- `metadata_cache: HashMap<sop_uid, Arc<Vec<TagRow>>>` — flattened tag rows for the metadata panel, also on-demand.
- `cells: Vec<CellState>` + `grid: GridLayout` + `active_cell: usize` — the viewport grid. **Every per-image piece of view state (pan, zoom, rotation_quarter, flip_h/v, invert, window, in-progress annotation) lives on the cell, not the instance.** Switching grid layout preserves cells in array order and truncates extras.
- `annotation_store: AnnotationStore` — annotations persisted to `<data_dir>/annotations.json`, keyed by SOP Instance UID. Saved synchronously on every push/undo/clear. **Original DICOM files are never modified.**

`CellState.tex` is an `egui::TextureHandle` rendered with current window/level. `tex_uid` / `tex_window` / `tex_invert` track which rendering it represents — `ensure_texture` in `ui/viewport.rs` compares them and regenerates the RGBA via `dcm::render_rgba` only when one of them drifts.

### Portable-first paths (`src/config.rs`)

`Paths::resolve()` probes the exe directory; if writable, the app runs in "portable mode" and stores `config.toml`, `annotations.json`, and `logs/` next to the binary. Otherwise it falls back to a platform user-data directory (via `directories` crate). The status bar exposes which mode is active. Code that writes anywhere persistent must use `paths.data_dir`, never assume CWD.

### DICOM pipeline (`src/dcm/`)

- `loader.rs` walks a folder in parallel (rayon), reads each candidate with `OpenFileOptions::read_until(PIXEL_DATA)` — **never decodes pixels during the scan** — then groups into Study/Series. Recognises files by `.dcm`/`.dicom` extension or by probing the `DICM` magic at byte 128 (CD-style filenames with no extension).
- `pixel.rs` is the only place that decodes pixel data. Returns `RawImage` containing rescaled (modality-LUT-applied) `f32` values with **no VOI applied** — interactive windowing reapplies a LUT on every change in `render_rgba`. Images larger than `MAX_DISPLAY_DIM` (2048) are **decimated at load time**; `RawImage.display_scale` records the factor so measurements still report real-world millimetres. Only monochrome photometric interpretations are fully supported.
- `roi.rs` computes rect/ellipse statistics over `RawImage.values` (already in HU for CT after modality LUT). `length_label` reads `Instance.pixel_spacing` and multiplies by `display_scale`.
- `annotation.rs` / `export.rs` / `metadata.rs` are self-contained. `export.rs` does its own CPU rasterisation (Bresenham + ellipse perimeter sampler) when burning annotations into PNGs, deliberately avoiding `imageproc` to keep binary size down. Only PNG output is enabled in the `image` crate features.

### UI composition (`src/ui/`)

Top-down: `menu_bar`, `toolbar`, optional `study_browser` (left), `viewport` (central), optional `metadata_panel` (right via `ui/dialogs/`), `status_bar`. `ui::draw` in `ui/mod.rs` is the single entry point called by `App::update`.

`viewport.rs` is the densest file. Each cell paints itself, then a separate pass checks `DragAndDrop::payload::<SeriesDragPayload>` (we do **not** use `dnd_drop_zone` because painted-not-allocated content makes the zone collapse — see commit history). `compute_dst_rect` + `image_to_screen` / `screen_to_image` implement the cell transform pipeline (flip → rotation → zoom → pan); annotations and the in-progress sketch must go through these to stay aligned with the image when rotated/flipped.

The toolbar's measurement tools (Length / Angle / Rect ROI / Ellipse ROI) coexist with W/L / Pan / Zoom. Switching to a measurement tool clears `ui_state.in_progress`. Mouse handling in `viewport::handle_input` dispatches to `handle_measurement` when `tool.is_measurement()`; middle-mouse pan stays active in every tool.

### Crash diagnostics

Release builds use `panic = "abort"`. `main.rs` installs a panic hook that writes the panic location, message, and a captured backtrace to the tracing log **before** the process aborts — so when a user reports a crash, the bottom of `logs/dicom-viewer.log.<date>` tells you exactly where to look. Don't remove that hook.

## Decisions that are locked in (don't change without asking)

- **UI**: `egui` via `eframe` with the `wgpu` renderer feature. No system deps, statically linked. eframe's `glow` feature is **not** enabled — `App::on_exit` takes `&mut self` only (the alternate signature with `Option<&glow::Context>` is gated behind the glow feature).
- **DICOM**: `dicom-rs` 0.7 family (`dicom`, `dicom-pixeldata`, `dicom-dictionary-std`). Sequences are surfaced as `<sequence, N item(s)>` in the metadata panel; no recursion yet.
- **Concurrency**: Decode is on the UI thread. There is no `tokio` runtime. MG-sized images are downsampled, not threaded.
- **Image crate features**: `default-features = false, features = ["png"]` only. Adding JPEG / WebP roughly doubles dependency compile time and adds ~1 MB.
- **`panic = "abort"`** in release. Keep the panic hook.
- **Release profile**: `lto = "fat"`, `codegen-units = 1`, `strip = true`. The 30 MB binary budget is real; check `ls -lh target/release/dicom-viewer` after adding dependencies.

## Things easy to break

- **Texture cache invalidation.** If you add a new visual modifier (e.g. gamma), it must participate in the `(tex_uid, tex_window, tex_invert)` comparison in `ensure_texture`, or stale textures will show.
- **Measurement coordinate space.** Annotations are in displayed-image-pixel coords (post-`display_scale`). Anything that converts to real-world units must multiply by `pixel_spacing × display_scale`. The `RoiStats.label` helper takes modality so it can suffix HU only on CT.
- **MG hanging protocol.** `app::select_series` auto-switches to a 2×2 grid and sorts views as RCC, LCC, RMLO, LMLO when the series is mammography with ≥4 instances. Dragging a series onto a specific cell skips this — it just drops the first instance into that cell.
- **Drag-and-drop in egui 0.29.** The sidebar uses `dnd_drag_source` and the inner `selectable_label`'s response is what carries click sense (`inner.inner.clicked()`); the outer response has only hover+drag. The viewport reads the payload via `DragAndDrop::payload::<SeriesDragPayload>` and consumes it with `clear_payload` on release — do not wrap cells in `dnd_drop_zone`.

## Roadmap status (per the project prompt)

Milestones 1–4 (skeleton, DICOM loading, study tree, viewing tools) and 6–8 (metadata panel, measurement tools, export+anonymise) are implemented. **Milestone 5 (CD/DVD auto-detection) and Milestone 9 (IMAPI2 burning)** are not started; both are gated on Windows testing and will require `#[cfg(target_os = "windows")]` modules under `src/optical/`.

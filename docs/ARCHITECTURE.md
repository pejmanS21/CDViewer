# Architecture

This document is a tour of the CDViewer codebase aimed at new contributors.
For the day-to-day developer workflow (build, test, lint, hooks), see
[`CONTRIBUTING.md`](../CONTRIBUTING.md).

## Scope of the project

CDViewer is a **portable, non-diagnostic** DICOM viewer in Rust, primarily
targeting Windows but with first-class Linux and macOS support. The viewer is
intended for review and reference; a "Not for diagnostic use" disclaimer
fires on first run and is persisted in `config.toml`.

The only OS-specific code path that is *currently* unimplemented is optical
media auto-detection and disc burning on Windows.

## Library + binary split

```
src/
├── lib.rs           # `pub mod app; config; dcm; logging; ui;`
├── main.rs          # Thin: paths, logging, panic hook, eframe boot
├── app.rs           # `DicomViewerApp` — the single eframe::App
├── config.rs        # `Paths::resolve()`, config.toml load/save
├── logging.rs       # tracing + tracing-appender setup
├── dcm/             # DICOM I/O — loader, pixel, ROI, export, …
└── ui/              # egui widgets — menu, toolbar, viewport, dialogs, …
```

[`src/lib.rs`](../src/lib.rs) exposes every module so the integration tests
in [`tests/sample_data.rs`](../tests/sample_data.rs) can call into the same
code that the binary uses. **Anything that needs to be tested must live in
the library**, not in `main.rs`.

If you add a new top-level module, expose it from `lib.rs`.

## State lives in `DicomViewerApp`

There is exactly one [`eframe::App`](../src/app.rs) and it owns all UI
state. Single-threaded, no async runtime. Decode is on the UI thread.

The important fields:

| Field | What it holds |
|---|---|
| `studies: Vec<Study>` | The loaded metadata tree (Study → Series → Instance). Pixels are **not** here. |
| `raw_cache: HashMap<sop_uid, Arc<RawImage>>` | Decoded pixels keyed by SOP Instance UID. Filled on demand by `dcm::load_raw`. Cleared on `open_folder`. |
| `metadata_cache: HashMap<sop_uid, Arc<Vec<TagRow>>>` | Flattened tag rows for the metadata panel; also lazy. |
| `cells: Vec<CellState>` | The viewport grid cells. |
| `grid: GridLayout` | Current layout (1×1 up to 4×4). |
| `active_cell: usize` | Which cell receives toolbar actions and keyboard input. |
| `annotation_store: AnnotationStore` | Persisted to `<data_dir>/annotations.json`. |

### Cells own view state, not instances

Every per-image piece of view state — **pan, zoom, rotation_quarter, flip_h,
flip_v, invert, window, in-progress annotation** — lives on the `CellState`,
not on the DICOM instance. This is intentional: the same image can be open
in two cells with different windowing or rotation.

Switching grid layout preserves cells in array order and truncates extras.

### Texture cache and invalidation

`CellState.tex` is an `egui::TextureHandle` of the current windowed RGBA
render. Three tracking fields decide whether to rebuild it:

- `tex_uid` — which SOP Instance the texture represents.
- `tex_window` — the window/level used to render it.
- `tex_invert` — whether photometric inversion was applied.

[`ui/viewport.rs::ensure_texture`](../src/ui/viewport.rs) compares these to
the current cell state and calls `dcm::render_rgba` to regenerate only when
one drifts.

> **If you add a new visual modifier** (e.g. gamma, presentation LUT), it
> must participate in this comparison or the texture cache will go stale.

## DICOM pipeline (`src/dcm/`)

| File | Responsibility |
|---|---|
| `loader.rs` | Parallel folder scan (rayon). Reads with `OpenFileOptions::read_until(PIXEL_DATA)` — **never decodes pixels during the scan**. Recognises files by extension or by probing the `DICM` magic at byte 128. Groups into Study/Series. |
| `pixel.rs` | The **only** place pixel data is decoded. Returns `RawImage` containing rescaled (modality-LUT-applied) `f32` values with **no VOI applied**. Interactive windowing re-applies a LUT on every change in `render_rgba`. Images larger than `MAX_DISPLAY_DIM` (2048) are decimated at load time; `RawImage.display_scale` records the factor so measurements still report real-world millimetres. |
| `roi.rs` | Rect / ellipse statistics over `RawImage.values` (already in HU for CT after modality LUT). `length_label` reads `Instance.pixel_spacing` and multiplies by `display_scale`. |
| `annotation.rs` | Annotation model + sidecar JSON serialisation. |
| `export.rs` | PNG export. Does its own CPU rasterisation (Bresenham + ellipse perimeter sampler) for burned-in annotations — deliberately avoids `imageproc` to keep binary size down. |
| `metadata.rs` | Flattens a `DefaultDicomObject` into the `TagRow` list shown in the metadata panel. Sequences are surfaced as `<sequence, N item(s)>` placeholders. |
| `study.rs` | `Study`/`Series`/`Instance` data model. |
| `thumbnail.rs` | Thumbnail decode for the study browser. |

> **Only monochrome photometric interpretations are fully supported.** Colour
> DICOMs render but are not rigorously tested.

### Measurement coordinate space

Annotations are stored in **displayed-image-pixel coordinates** (post
`display_scale`). Anything that converts to real-world units must multiply
by `pixel_spacing × display_scale`. The `RoiStats::label` helper takes
modality so it suffixes HU only on CT.

## UI composition (`src/ui/`)

```
ui::draw
├── menu_bar           # File / View / Tools / Help
├── toolbar            # W/L, Pan, Zoom, Length, Angle, Rect ROI, Ellipse ROI
├── study_browser      # Left, optional. Drag source for series.
├── viewport           # Central. Multi-cell grid.
├── dialogs/           # Metadata panel (right), export modal, etc.
└── status_bar         # Portable-vs-user-data mode, FPS, message.
```

[`ui::draw`](../src/ui/mod.rs) in `ui/mod.rs` is the single entry point
called by `App::update`.

[`ui/viewport.rs`](../src/ui/viewport.rs) is the densest file. The flow:

1. Each cell paints itself: image texture, annotation overlays, in-progress
   sketch, ROI labels.
2. A separate pass checks `DragAndDrop::payload::<SeriesDragPayload>`. We do
   **not** use `dnd_drop_zone` — painted-not-allocated content makes the
   zone collapse (see commit history).
3. `compute_dst_rect` + `image_to_screen` / `screen_to_image` implement the
   cell transform pipeline (flip → rotation → zoom → pan). Annotations and
   the in-progress sketch must go through these to stay aligned when the
   image is rotated/flipped.
4. Mouse handling in `viewport::handle_input` dispatches to
   `handle_measurement` when the tool is a measurement tool. Middle-mouse
   pan stays active in every tool.

Switching to a measurement tool clears `ui_state.in_progress`.

### MG hanging protocol

[`app::select_series`](../src/app.rs) auto-switches to a 2×2 grid and sorts
views as **RCC, LCC, RMLO, LMLO** when the series is mammography with ≥4
instances. Dragging a series onto a *specific* cell skips this — it just
drops the first instance into that cell.

### Drag-and-drop quirks (egui 0.29)

The sidebar uses `dnd_drag_source`. The inner `selectable_label`'s response
carries click sense (`inner.inner.clicked()`); the outer response only has
hover + drag. The viewport reads the payload via
`DragAndDrop::payload::<SeriesDragPayload>` and consumes it with
`clear_payload` on release. **Do not wrap cells in `dnd_drop_zone`.**

## Portable-first paths

[`src/config.rs::Paths::resolve()`](../src/config.rs) probes the executable
directory; if writable, the app runs in **portable mode** and stores
`config.toml`, `annotations.json`, and `logs/` next to the binary.
Otherwise it falls back to a platform user-data directory (via the
`directories` crate):

- Windows: `%APPDATA%\dicom-viewer\`
- macOS: `~/Library/Application Support/dicom-viewer/`
- Linux: `~/.local/share/dicom-viewer/`

The status bar shows which mode is active. Any code that writes anywhere
persistent **must** use `paths.data_dir` — never assume CWD.

## CD/DVD auto-load

Priority is defined in [`src/main.rs::detect_startup_folder`](../src/main.rs):

1. `argv[1]` if it exists on disk.
2. `$DICOM_VIEWER_DATA` env var if set and exists.
3. Sibling `DICOM/` / `dicom/` / `IMAGES/` / `images/` / `DICOMDIR/` next
   to the executable.
4. Otherwise the viewer launches empty.

The load happens on the **first `update` tick** (not in `App::new`) so the
window paints once before a potentially slow optical-media scan starts. The
pending folder lives on the app struct as `pending_startup`, alongside
`pending_drops` for runtime drag-drop loads.

See [`dist/CD-DISTRIBUTION.md`](../dist/CD-DISTRIBUTION.md) for the disc
layout and staging scripts.

## Crash diagnostics

Release builds use `panic = "abort"`. [`main.rs`](../src/main.rs) installs a
panic hook that writes the panic location, message, and a captured backtrace
to the tracing log **before** the process aborts — so when a user reports a
crash, the bottom of `<data_dir>/logs/dicom-viewer.log.<date>` shows exactly
where to look.

> **Don't remove the panic hook.**

## Decisions that are locked in

These have been deliberately chosen and tested. Don't change them without
discussion.

- **UI**: `egui` via `eframe` with the `wgpu` renderer feature. No system
  deps, statically linked. eframe's `glow` feature is **not** enabled —
  `App::on_exit` takes `&mut self` only.
- **DICOM**: `dicom-rs` 0.7 family. Sequences are surfaced as
  `<sequence, N item(s)>` in the metadata panel; no recursion yet.
- **Concurrency**: Decode is on the UI thread. There is no `tokio`. MG-sized
  images are downsampled, not threaded.
- **`image` crate features**: `default-features = false, features = ["png"]`
  only. Adding JPEG/WebP roughly doubles dependency compile time and adds
  ~1 MB to the binary.
- **`panic = "abort"`** in release. Keep the panic hook.
- **Release profile**: `lto = "fat"`, `codegen-units = 1`, `strip = true`,
  `opt-level = "z"`. The ~30 MB binary budget is real — check
  `ls -lh target/release/dicom-viewer` after adding dependencies.

## Roadmap

Per the original project prompt, milestones 1–4 (skeleton, DICOM loading,
study tree, viewing tools) and 6–8 (metadata panel, measurement tools,
export + anonymise) are implemented. **Milestone 5 (CD/DVD auto-detection)**
and **Milestone 9 (IMAPI2 burning)** are not started; both are gated on
Windows testing and will require `#[cfg(target_os = "windows")]` modules
under `src/optical/`.

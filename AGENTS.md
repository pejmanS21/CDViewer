# AGENTS.md

## What this is

CDViewer (binary/crate name `dicom-viewer`) is a portable, single-executable,
**non-diagnostic** DICOM viewer written in Rust (edition 2021, MSRV 1.80),
using `egui`/`eframe` 0.29 (`glow`/OpenGL renderer) and the `dicom-rs` 0.7 family.
Ships as the only software on a study CD/DVD and auto-loads a sibling
`DICOM/` folder; a "Not for diagnostic use" disclaimer shows on first run.
Pure Cargo project — no npm/pip/uv/bun anywhere in this repo.

## Commands

Run from the repo root (`Cargo.toml` lives at top level).

```bash
cargo run                                                  # dev build+run; logs to <exe_dir>/logs/dicom-viewer.log.<date>
cargo build --release                                      # what users get; ~30 MB stripped budget
cargo build --profile release-fast                         # thin-LTO local iteration, faster link, bigger binary
cargo clippy --all-targets --all-features -- -D warnings   # must stay clean — same gate as CI
cargo test                                                 # tests/sample_data.rs auto-skips if sample-data/ is absent
cargo test rect_roi_stats_on_ct                             # single test by substring
RUST_LOG=dicom_viewer=debug,egui_glow=warn cargo run        # tracing filter at runtime
cargo doc --no-deps --lib --open                            # rustdoc site (same flags as docs.yml)
```

CI/release build **only** `x86_64-pc-windows-msvc` — that is the sole shipped target. On Linux, `eframe` still needs system packages for local dev — see the `apt-get install` line in `README.md` before building.

`sample-data/ct-data/` (30 CT slices) and `sample-data/mg-data/` (4 MG views) are the fixtures the integration tests and manual smoke tests use; not required to be present (tests skip cleanly without them).

## Architecture

Full tour: `docs/ARCHITECTURE.md`. CI/release matrix and how to cut a release: `docs/CI-CD.md`. Both are kept current — read them before non-trivial changes. This section is only the parts most likely to trip up a change:

- `src/lib.rs` exposes every module; `src/main.rs` is thin startup wiring only (paths, logging, panic hook, eframe boot). **Integration tests go through the library** — anything they touch must be `pub mod` in `lib.rs`, not left in `main.rs`.
- All state lives on `DicomViewerApp` (`src/app.rs`): `studies` (metadata tree, no pixels), `raw_cache`/`metadata_cache` (lazy per-SOP-UID caches, cleared on `open_folder`), `cells: Vec<CellState>` (the viewport grid — **every per-image transform, pan/zoom/rotation/flip/invert/window, lives on the cell, not the instance**), `annotation_store` (synced to `<data_dir>/annotations.json` synchronously on every push/undo/clear).
- DICOM pipeline is `src/dcm/`: `loader.rs` scans a folder without decoding pixels; `pixel.rs` is the only place pixel data is decoded (`RawImage`, no VOI applied, decimated at `MAX_DISPLAY_DIM` = 2048px, recorded as `display_scale`); `roi.rs` computes stats over `RawImage.values`.
- `src/ui/viewport.rs::ensure_texture` is the densest function: the texture cache keys off `(tex_uid, tex_window, tex_invert)` — any new visual modifier (e.g. gamma) must join that comparison or textures go stale.
- Paths (`src/config.rs::Paths::resolve()`): portable mode (writable exe dir) vs. platform user-data fallback. Anything persistent must use `paths.data_dir`, never assume CWD.
- CD auto-load priority (`src/main.rs::detect_startup_folder`): `argv[1]` → `$DICOM_VIEWER_DATA` → sibling `DICOM/`|`dicom/`|`IMAGES/`|`images/`|`DICOMDIR/` next to the exe. Runs on the first `update` tick, not in `App::new`, so the window paints before a slow scan starts.

## Conventions & gotchas

- Measurement/annotation coordinates are stored in displayed-image-pixel space (post `display_scale`); convert to real-world mm via `pixel_spacing × display_scale` (see `roi::length_label`).
- egui 0.29 drag-and-drop: the sidebar (`study_browser.rs`) uses `dnd_drag_source`; the viewport reads `DragAndDrop::payload::<SeriesDragPayload>` and clears it manually — **do not** wrap viewport cells in `dnd_drop_zone` (it collapses on painted-not-allocated content).
- Mammography series with ≥4 instances auto-arrange into a 2×2 RCC/LCC/RMLO/LMLO grid (`app::select_series`, `try_arrange_mg_study`). Dragging a series onto one specific cell bypasses this and just drops the first instance there.
- Pre-commit (`.pre-commit-config.yaml`): `cargo fmt` runs on every commit; `clippy -D warnings` + `cargo test` only run on `pre-push`. Install both hook stages, not just the commit one.
- No network calls, no telemetry — the viewer must run fully offline, including from air-gapped/optical media (see `CONTRIBUTING.md` ground rules).
- Original DICOM files are never modified; annotations and exports only ever write to `data_dir` or an explicit export path.

## Don't touch without asking

- `panic = "abort"` and the panic hook installed in `main.rs` — crash log diagnostics depend on both.
- Release profile tuning in `Cargo.toml` (`lto`, `codegen-units`, `strip`, `opt-level`) and the `image` crate's `features = ["png"]`-only restriction — these hold the release binary to its size budget.
- `eframe`'s renderer stays `glow`, never `wgpu` — `wgpu` needs DX12 (Win10 + FL11 hardware) or Vulkan and fails to *launch* on the older/weaker PCs a study CD lands on. `App::on_exit` takes `(&mut self, Option<&eframe::glow::Context>)` because of it.
- Windows builds statically link the CRT (`.cargo/config.toml`, `+crt-static`) so the exe runs on a PC with no VC++ Redistributable. Don't drop it to save a few hundred KB.
- eframe's `persistence` feature stays off — config is TOML via `config.rs`, annotations are JSON via `dcm/annotation.rs`.
- CD/DVD auto-detection and IMAPI2 burning (roadmap milestones 5 and 9) are intentionally unimplemented, gated on Windows hardware testing — don't build features assuming they exist.

# Changelog

All notable changes to CDViewer are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.1.2] — 2026-08-03

### Added

- Viewport overlay now shows patient name, patient ID, and study instance UID
  in the top-centre corner, replacing the previous dead placeholder.

### Fixed

- Silenced `float_literal_f32_fallback` warnings across 24
  `egui::Stroke::new` call sites in 8 UI modules by explicitly suffixing
  float literals as `f32` — a newer stable toolchain now flags the previous
  implicit fallback as future-incompatible (rust-lang/rust#154024).

## [0.1.1] — 2026-07-29

### Performance

- Folder scan (`load_folder`) no longer reopens each DICOM file up to 3×
  to look up study/series UIDs and per-group metadata — everything is now
  read once, in the existing parallel pass. Matters most on slow optical
  (CD/DVD) media, where every extra file open costs a seek.
- Dropped a serial magic-byte probe on extensionless files during the
  folder walk in favour of `dicom-object`'s own fast preamble-parse
  failure, moving that check into the parallel pass.
- Decoded-pixel cache (`raw_cache`) is now bounded by a 256 MB byte
  budget (oldest evicted first) instead of growing without limit, so
  scrolling a long CT/MR stack can't exhaust memory on low-RAM Windows
  machines.
- Release `opt-level` raised from `"z"` to `3` for real execution speed
  (e.g. the per-pixel window/level render path), trading a modest binary
  size increase that's dwarfed by the DICOM payload shipped alongside the
  exe.

## [0.1.0] — 2026-05-22

Initial public release. CDViewer is a portable, non-diagnostic DICOM viewer
in Rust, designed to ship on study CDs/DVDs as a single executable.

### Highlights

- Single-executable distribution for Linux (x86_64 / aarch64), macOS (Apple
  Silicon), and Windows (x86_64 / aarch64) — no installer, no system
  dependencies on Windows/macOS.
- Targets a <30 MB stripped release binary (fat LTO, `opt-level = "z"`,
  `codegen-units = 1`, `strip = true`, `panic = "abort"`).
- Portable-first storage: config, annotations, and logs live next to the
  binary when possible; falls back to the per-user data directory for
  read-only media.
- Optional Windows `autorun.inf` for AutoPlay-driven launches from a study
  disc.

### Added

#### DICOM loading

- Parallel folder scan (rayon) that recognises files by `.dcm`/`.dicom`
  extension **or** by probing the `DICM` magic at byte 128 — works with
  CD-style filenames that have no extension.
- Metadata-only scan during folder load (`OpenFileOptions::read_until(PIXEL_DATA)`)
  with pixel data decoded on demand, cached by SOP Instance UID.
- Modality LUT applied at decode time; VOI/window is re-applied cheaply on
  every change without re-decoding.
- Automatic decimation of images larger than 2048 px (`MAX_DISPLAY_DIM`) so
  large MG/CR studies stay responsive. `display_scale` is preserved so
  measurements still report real-world millimetres.

#### Viewing

- Multi-cell grid layouts from 1×1 up to 4×4. Per-cell pan, zoom, rotation
  (quarter-turn), horizontal/vertical flip, invert, and window/level.
- Window/level presets: soft tissue (40/400), lung (-600/1500), bone
  (300/1500), brain (40/80), abdomen (60/400), mediastinum (50/350).
- Drag-and-drop a series from the study browser onto a specific cell.
- Mammography hanging protocol: auto-switches to a 2×2 grid and sorts views
  as RCC / LCC / RMLO / LMLO when an MG series has ≥4 instances.
- Keyboard navigation: Page Up/Down and arrow keys to scroll slices,
  Home/End to jump to first/last.
- Ctrl/⌘+scroll to zoom, scroll wheel to advance slices, middle-mouse drag
  to pan in any tool.

#### Measurement

- Length tool — millimetres when `PixelSpacing` is present.
- Angle tool — three-click vertex measurement.
- Rectangle ROI — mean / std / min / max (in HU for CT after modality LUT).
- Ellipse ROI — same statistics as rectangle ROI.
- Annotations persisted to `<data_dir>/annotations.json`, keyed by SOP
  Instance UID. Original DICOM files are never modified.

#### Metadata & export

- Full DICOM tag tree with right-click copy (value / tag / name). Sequences
  are surfaced as `<sequence, N item(s)>` placeholders.
- PNG export with annotations burned in via a self-contained CPU
  rasteriser (Bresenham + ellipse perimeter sampler) — avoids pulling in
  `imageproc` to keep the binary small.
- Optional patient-data anonymisation on export.

#### CD/DVD distribution

- Auto-load priority: `argv[1]` → `$DICOM_VIEWER_DATA` → sibling
  `DICOM/`/`dicom/`/`IMAGES/`/`images/`/`DICOMDIR/` next to the executable.
- The startup scan runs on the first `update` tick (not in `App::new`) so
  the window paints before a slow optical-media scan begins.
- Staging scripts: [`dist/make-cd.sh`](dist/make-cd.sh) (POSIX) and
  [`dist/make-cd.ps1`](dist/make-cd.ps1) (Windows) produce a burnable
  `dist/cd-staging/` directory.

#### Reliability

- Release builds use `panic = "abort"` with a panic hook that writes the
  panic location, message, and backtrace to the rotating tracing log before
  the process aborts. Crash reports always have a starting point.
- Daily-rotated logs under `<exe_dir>/logs/dicom-viewer.log.<date>` (or the
  per-user data directory on read-only media).
- First-run "Not for diagnostic use" disclaimer, acknowledged once and
  persisted to `config.toml`.

#### Tooling & CI

- CI matrix across Linux x86_64/aarch64, macOS aarch64, Windows
  x86_64/aarch64 — runs `cargo clippy -- -D warnings` and `cargo test` on
  every PR/push. See [`docs/CI-CD.md`](docs/CI-CD.md).
- Tag-driven release workflow on `v*` tags that publishes archives for all
  five targets to a GitHub Release with auto-generated notes.
- Security workflow (Gitleaks + Semgrep) on push/PR plus a daily scheduled
  sweep.
- Docs workflow that builds `cargo doc` with `-D warnings` and deploys
  to GitHub Pages on every push to `main`.
- Pre-commit hooks: `cargo fmt` on commit, `cargo clippy` + `cargo test` on
  push.

#### Documentation

- Comprehensive crate-level rustdoc on every module and public item, with
  intra-doc links connecting the model (`Study`/`Series`/`Instance`), the
  pixel pipeline (`load_raw` → `render_rgba`), and the app state
  (`DicomViewerApp`, `CellState`).
- Long-form docs under `docs/`:
  [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the module map and design
  decisions, [`CI-CD.md`](docs/CI-CD.md) for the workflows, and
  [`dist/CD-DISTRIBUTION.md`](dist/CD-DISTRIBUTION.md) for the disc
  layout.
- Top-level [`README.md`](README.md), [`CONTRIBUTING.md`](CONTRIBUTING.md),
  and this [`CHANGELOG.md`](CHANGELOG.md).

### Known limitations

- **Optical-media auto-detection (Milestone 5)** and **IMAPI2 disc burning
  (Milestone 9)** are not implemented. Both are gated on Windows testing and
  will land under `src/optical/` behind `#[cfg(target_os = "windows")]`.
- Only monochrome photometric interpretations are fully supported. Colour
  DICOMs render but are not rigorously tested.
- `DICOMDIR` files are ignored — the recursive scan finds the same content.
- Multi-frame pixel data is decoded eagerly per frame; very large multi-frame
  series may be slow to first paint.
- DICOM sequences in the metadata panel are summarised as
  `<sequence, N item(s)>` with no recursion into the items.
- macOS x86_64 is not built in the release matrix (commented out in
  [`.github/workflows/release.yml`](.github/workflows/release.yml)). Apple
  Silicon and Rosetta cover macOS users.

### Compatibility

- Rust **1.80+** is required to build from source.
- Tested against the `dicom-rs` 0.7 family (`dicom`, `dicom-pixeldata`,
  `dicom-dictionary-std`).
- UI uses `eframe` / `egui` 0.29 with the `wgpu` renderer feature; the
  `glow` feature is intentionally **not** enabled.

[Unreleased]: https://github.com/pejmanS21/CDViewer/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/pejmanS21/CDViewer/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pejmanS21/CDViewer/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pejmanS21/CDViewer/releases/tag/v0.1.0

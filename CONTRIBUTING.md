# Contributing

Thanks for your interest in CDViewer. This document covers the local
development workflow, coding standards, and the PR process.

## Ground rules

- **Non-diagnostic, by design.** Anything that pushes the viewer toward a
  diagnostic role (DICOM-compliant measurements certified for clinical use,
  modifying the source DICOM files in place, multi-user workflows, etc.) is
  out of scope. Stick to review/reference features.
- **No telemetry, no network calls.** The viewer must run fully offline on
  air-gapped review workstations and from optical media.
- **Portable executable.** New dependencies must build cleanly on all five
  CI targets (Linux x86_64/aarch64, macOS aarch64, Windows x86_64/aarch64)
  and not bloat the release binary past the ~30 MB budget.

If a contribution would conflict with one of those, open an issue first.

## Prerequisites

- Rust **1.80** or newer (`rustup install stable`).
- On Linux, install the eframe system deps listed in the [README](README.md).
- Optional: [`pre-commit`](https://pre-commit.com/) for the local git hooks.

## Build & run

```bash
# Dev build — opens a window. Logs to <exe_dir>/logs/dicom-viewer.log.<date>
cargo run

# Release build — what users get. The 30 MB binary budget is real.
cargo build --release

# Fast local release build (thin LTO, ~40–50% faster link, slightly larger).
cargo build --profile release-fast

# Tracing filter
RUST_LOG=dicom_viewer=debug,wgpu_core=warn cargo run
```

The sample data under `sample-data/` (30-slice CT and 4-view MG) is the
canonical fixture for smoke tests and integration tests. It is **not** checked
into the repo — drop your own anonymised samples in if you want the
integration tests to run locally. They auto-skip when the directory is absent.

## Tests & linting

These are the same gates CI enforces.

```bash
# Lint — must stay clean. -D warnings is a CI gate.
cargo clippy --all-targets --all-features -- -D warnings

# All tests
cargo test

# A single test by substring
cargo test rect_roi_stats_on_ct
```

Integration tests live in [`tests/sample_data.rs`](tests/sample_data.rs) and
go through the library surface (`src/lib.rs`) — they cannot reach into
anything that lives only in `main.rs`. If you add a new top-level module,
make sure it is `pub mod`-exported from `lib.rs`.

## Building the API docs

The library exposes rustdoc comments on every public item; CI publishes
them to GitHub Pages on every push to `main`.

```bash
# Build and open the rustdoc site locally
cargo doc --no-deps --lib --open

# Treat any broken intra-doc link as a hard error (matches CI)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --lib
```

The published site lives at <https://pejmanS21.github.io/CDViewer/>.
See [`docs/CI-CD.md`](docs/CI-CD.md#docs-workflow) for the deploy details.

When you add new public items, write at least a one-sentence `///`
doc — link related items with intra-doc links (e.g. `[`CellState`]`) so
the rustdoc navigation stays connected.

## Pre-commit hooks

```bash
pip install pre-commit
pre-commit install                       # fmt on every commit
pre-commit install --hook-type pre-push  # clippy + tests on push
```

`cargo fmt` runs on every commit (cheap). `cargo clippy -- -D warnings` and
`cargo test` only fire on `git push` so day-to-day commits stay snappy. See
[`.pre-commit-config.yaml`](.pre-commit-config.yaml).

## Codebase orientation

For a deeper map of the architecture, read
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). The short version:

```
src/
├── lib.rs           # Library surface used by integration tests
├── main.rs          # Thin binary: startup wiring + eframe boot
├── app.rs           # Single eframe::App. Owns all UI state.
├── config.rs        # Portable-first path resolution + config.toml
├── logging.rs       # tracing + tracing-appender setup
├── dcm/             # DICOM I/O — loader, pixel decode, ROI, export
└── ui/              # egui widgets — menu/toolbar/viewport/dialogs/...
```

State lives in `DicomViewerApp` (single-threaded UI). Pixels are decoded
lazily, cached by SOP Instance UID, and re-windowed cheaply on every change.
Per-image view state (pan/zoom/rotation/window/in-progress measurement) lives
on the **cell**, not the **instance** — this is intentional so the same image
can be open twice with different settings.

## Coding standards

- `cargo fmt` defaults. The pre-commit hook enforces this.
- No `unsafe` unless you describe the invariant in a comment block above it.
- Prefer `anyhow::Result` for fallible app code; `thiserror` for typed errors
  that cross module boundaries.
- Logging is `tracing::{info,warn,error,debug}` macros — no `println!` in
  release-path code.
- Avoid adding `image` crate features. The Cargo.toml comment explains why
  (~doubles dependency compile time, +1 MB to the binary).
- Avoid adding async runtimes. There is intentionally no `tokio`.

### Easy ways to break things (read these)

These are covered in the architecture doc, but worth repeating:

- **Texture cache invalidation.** Any new visual modifier (e.g. gamma) must
  participate in the `(tex_uid, tex_window, tex_invert)` comparison in
  `ui/viewport.rs::ensure_texture`, or stale textures will render.
- **Measurement coordinate space.** Annotations live in displayed-image-pixel
  coords (post-`display_scale`). Real-world units multiply by
  `pixel_spacing × display_scale`.
- **MG hanging protocol.** `app::select_series` auto-switches to a 2×2 grid
  for mammography series with ≥4 views; drag-onto-cell bypasses this.
- **Drag-and-drop in egui 0.29.** The sidebar uses `dnd_drag_source` + the
  inner `selectable_label`'s response for clicks. The viewport reads payloads
  via `DragAndDrop::payload::<SeriesDragPayload>` and consumes them with
  `clear_payload` on release — do **not** wrap cells in `dnd_drop_zone`.

## Submitting changes

1. Fork and create a feature branch off `main`.
2. Make focused commits — bug fix vs. feature in separate commits where
   possible. Commit messages should explain *why*, not *what*.
3. Make sure `cargo fmt`, `cargo clippy --all-targets --all-features --
   -D warnings`, and `cargo test` all pass.
4. Open a PR against `main`. The CI matrix runs clippy + tests on all five
   targets; a green build is required before review.
5. The security workflow (Gitleaks + Semgrep) also runs on PRs. False
   positives can be silenced in `.semgrepignore`.

If your change touches the user-facing surface (a tool, a menu entry, an
exported file format, an auto-load rule), update:

- [`CHANGELOG.md`](CHANGELOG.md) under `## [Unreleased]`.
- [`dist/HELP.txt`](dist/HELP.txt) if it adds a shortcut or tool.
- [`dist/README.txt`](dist/README.txt) if it changes how end-users interact
  with the disc layout.

## Reporting bugs / requesting features

Please open a [GitHub Issue](https://github.com/pejmanS21/CDViewer/issues).
For crashes, attach the relevant lines from
`<exe_dir>/logs/dicom-viewer.log.<date>` — the panic hook captures the
location, message, and a backtrace.

## Security

If you find a vulnerability, please **do not** open a public issue. See the
note in [`docs/CI-CD.md`](docs/CI-CD.md#security-workflow) about the scanning
setup, and email the maintainer privately.

## Licence

By submitting a contribution you agree that it will be licensed under the
[Apache License 2.0](LICENSE), the same licence as the rest of the project.

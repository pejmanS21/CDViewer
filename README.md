# CDViewer — Portable DICOM Viewer

[![CI](https://github.com/pejmanS21/CDViewer/actions/workflows/ci.yml/badge.svg)](https://github.com/pejmanS21/CDViewer/actions/workflows/ci.yml)
[![Release](https://github.com/pejmanS21/CDViewer/actions/workflows/release.yml/badge.svg)](https://github.com/pejmanS21/CDViewer/actions/workflows/release.yml)
[![Docs](https://github.com/pejmanS21/CDViewer/actions/workflows/docs.yml/badge.svg)](https://pejmanS21.github.io/CDViewer/)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)

A portable, single-executable DICOM viewer written in Rust. Designed to ship on
study CDs/DVDs alongside the imaging data and run without installation on
Windows, macOS, and Linux.

> **Not for diagnostic use.** This viewer is intended for review and reference
> only. It must not be used as the basis for primary clinical diagnosis. A
> disclaimer is shown on first launch and acknowledged in `config.toml`.

---

## Features

- **Single executable** — statically linked, no installer, no system
  dependencies on Windows/macOS. Targets a <30 MB stripped binary.
- **Portable-first storage** — config, annotations, and logs are written next
  to the executable when possible; falls back to the per-user data directory
  for read-only media (CD/DVD).
- **CD/DVD auto-load** — drop a `DICOM/` folder next to the binary; on
  Windows, `autorun.inf` opens the study automatically.
- **Modality support** — CT, MR, CR, DX, and MG (with a 2×2 hanging
  protocol for RCC/LCC/RMLO/LMLO when ≥4 mammography views are present).
- **Viewing tools** — window/level (with presets for soft tissue, lung, bone,
  brain, abdomen, mediastinum), pan, zoom, rotate, flip, invert, multi-cell
  grid layouts (1×1 up to 4×4).
- **Measurement tools** — length (mm when pixel spacing is present), angle,
  rectangle ROI, ellipse ROI with mean/std/min/max (HU on CT).
- **Annotations** — persisted to a JSON sidecar, keyed by SOP Instance UID.
  Original DICOM files are never modified.
- **Export** — PNG with annotations burned in, with optional patient-data
  anonymisation.
- **Metadata panel** — full DICOM tag tree with right-click copy.
- **Crash-friendly** — a panic hook writes the panic location, message, and
  backtrace to the log file before the process aborts.

## Screenshots

_Add screenshots to `docs/images/` and reference them here._

## Install

### Download a release

Pre-built binaries are attached to every [GitHub Release](https://github.com/pejmanS21/CDViewer/releases):

| Platform | Archive |
|---|---|
| Linux x86_64 | `dicom-viewer-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux aarch64 | `dicom-viewer-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `dicom-viewer-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `dicom-viewer-vX.Y.Z-x86_64-pc-windows-msvc.zip` |
| Windows aarch64 | `dicom-viewer-vX.Y.Z-aarch64-pc-windows-msvc.zip` |

Extract and run the binary directly. No installation step is required.

### Build from source

Requires Rust 1.80+.

```bash
git clone https://github.com/pejmanS21/CDViewer.git
cd CDViewer
cargo build --release
./target/release/dicom-viewer
```

Releases ship only `x86_64-pc-windows-msvc`; other platforms are build-from-source
for development. On Linux, the eframe backend needs a few system packages:

```bash
sudo apt-get install -y \
  libgtk-3-dev libxkbcommon-dev libxkbcommon-x11-0 \
  libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libssl-dev pkg-config
```

## Usage

```bash
# Open a folder of DICOM files
dicom-viewer /path/to/study

# Or set an env var (used by the CD auto-load path)
DICOM_VIEWER_DATA=/path/to/study dicom-viewer

# Or launch empty and drag-drop a folder onto the window
dicom-viewer
```

When the executable lives next to a folder named `DICOM/`, `dicom/`,
`IMAGES/`, `images/`, or `DICOMDIR/`, that folder is loaded automatically on
startup.

See [`dist/HELP.txt`](dist/HELP.txt) for the full mouse/keyboard reference.

## Distributing on CD/DVD

This viewer is designed to ship as the only software on a study disc.
See [`dist/CD-DISTRIBUTION.md`](dist/CD-DISTRIBUTION.md) for the disc layout,
auto-load rules, staging scripts (`dist/make-cd.sh`, `dist/make-cd.ps1`), and
ISO-creation commands.

## Documentation

- **[API docs (rustdoc)](https://pejmanS21.github.io/CDViewer/)** —
  generated from the source on every push to `main`. The crate root has
  the module map and architecture overview; individual items have their
  own pages.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development setup, coding standards,
  pre-commit hooks, PR workflow.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — module layout, state model,
  DICOM pipeline, UI composition, locked-in design decisions.
- [`docs/CI-CD.md`](docs/CI-CD.md) — how `ci.yml`, `release.yml`,
  `security.yml`, and `docs.yml` work; how to cut a release.
- [`dist/CD-DISTRIBUTION.md`](dist/CD-DISTRIBUTION.md) — disc layout,
  auto-load priority, staging, ISO commands, cross-compilation.
- [`CHANGELOG.md`](CHANGELOG.md) — release history.

### Generate the docs locally

```bash
cargo doc --no-deps --lib --open
```

This builds the rustdoc site (same flags as CI) and opens
`target/doc/dicom_viewer/index.html` in your browser. Add
`RUSTDOCFLAGS="-D warnings"` to fail on broken intra-doc links the same
way the [docs workflow](docs/CI-CD.md#docs-workflow) does.

## Licence

Licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project shall be licensed as above, without
any additional terms or conditions.

## Acknowledgements

Built on top of the excellent [`dicom-rs`](https://github.com/Enet4/dicom-rs)
ecosystem and the [`egui`](https://github.com/emilk/egui) / `eframe`
immediate-mode GUI framework.

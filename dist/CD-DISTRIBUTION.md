# CD/DVD Distribution

The viewer is built to ship as a single executable on a study disc. When
the disc is inserted, the user gets one of:

- **Windows AutoPlay prompt** → "Open DICOM study with DICOM Viewer" →
  the viewer launches and auto-loads `DICOM/`.
- **Double-click on the disc icon** → executes `dicom-viewer.exe DICOM`
  (Windows) or `Run viewer.command` (macOS).
- **CLI** → `dicom-viewer /path/to/study` works on every OS.

## Auto-loading rules (in priority order)

Defined in [`src/main.rs::detect_startup_folder`](../src/main.rs):

1. `argv[1]` if it exists on disk.
2. `$DICOM_VIEWER_DATA` env var if set and exists.
3. Sibling `DICOM/`, `dicom/`, `IMAGES/`, `images/`, `DICOMDIR/` next to
   the executable.
4. Otherwise the viewer launches empty.

The load happens on the first `update` tick (not in `App::new`) so the
window paints once before a potentially slow optical-media scan starts.

## Disc layout

```
D:\
├── autorun.inf            ← Windows AutoPlay manifest
├── dicom-viewer.exe       ← Windows binary
├── dicom-viewer           ← optional macOS / Linux binary
├── Run viewer.command     ← optional macOS double-click launcher
├── README.txt             ← end-user instructions
├── HELP.txt               ← keyboard / mouse cheat-sheet
├── LICENSE.txt
└── DICOM\                 ← study data the viewer auto-loads
    ├── series-1\
    │   ├── 0001.dcm
    │   └── …
    └── series-2\
        └── …
```

The viewer's folder scan recognises files by `.dcm` extension *or* by
probing the `DICM` magic at byte 128, so CD-style filenames with no
extension also work. `DICOMDIR` files are currently ignored — the
recursive scan finds the same content.

## Building a staging directory

POSIX:

```bash
dist/make-cd.sh                    # uses the host-platform release build
dist/make-cd.sh /path/to/study     # also stages the DICOM data
```

Windows PowerShell:

```powershell
dist\make-cd.ps1
dist\make-cd.ps1 -DicomSource C:\path\to\study
```

Both scripts produce `dist/cd-staging/`. Drop additional DICOM files into
`dist/cd-staging/DICOM/` and burn the staging directory directly, or wrap
it in an ISO:

```bash
genisoimage -V DICOM_VIEWER -J -r -o dicom-viewer.iso dist/cd-staging   # Linux
hdiutil makehybrid -iso -joliet -default-volume-name DICOM_VIEWER \
        -o dicom-viewer.iso dist/cd-staging                              # macOS
oscdimg -j1 -lDICOM_VIEWER dist\cd-staging dicom-viewer.iso              # Windows
```

## Read-only volumes

When the executable is on a read-only filesystem (CD, write-protected
USB), `Paths::resolve()` falls back to the per-user data directory:

- Windows: `%APPDATA%\dicom-viewer\`
- macOS: `~/Library/Application Support/dicom-viewer/`
- Linux: `~/.local/share/dicom-viewer/`

That's where `config.toml`, `annotations.json`, and `logs/` go when the
binary can't write next to itself. The disclaimer-acknowledgement is
therefore remembered across CD insertions on the same machine.

## Cross-compilation (building the Windows binary on macOS/Linux)

Install the target and a linker:

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64     # macOS
sudo apt install mingw-w64 # Debian/Ubuntu
cargo build --release --target x86_64-pc-windows-gnu
```

Then point `make-cd.sh` at the cross-compiled binary by copying it to
`target/release/dicom-viewer.exe`, or extend the script.

## Optional polish (not implemented)

- **Embedded icon** — add `winres = "0.1"` as a build-dep, drop an `.ico`
  in `dist/`, write a `build.rs` that calls `WindowsResource::new()
  .set_icon("dist/icon.ico").compile()`. Adds ~50 KB to the binary and
  gives the exe a proper Explorer thumbnail + an icon for `autorun.inf`.
- **DICOMDIR fast path** — parse `DICOMDIR` first when present, skipping
  the recursive scan. Pays off on multi-GB optical media; immaterial on
  a hundred-slice study.
- **Code-signing** — Windows SmartScreen will warn on an unsigned exe
  the first time a user runs it from a CD. Signing with an EV cert
  removes the warning.

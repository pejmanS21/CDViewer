//! Portable-first config and path resolution.
//!
//! Settings live in `config.toml` next to the executable. If the exe
//! directory is read-only (e.g. running from a CD), we fall back to a
//! per-user data directory and remember that decision for the session.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.toml";

/// Resolved filesystem locations for the running viewer.
///
/// Construct with [`Self::resolve`]. In *portable mode* the data and log
/// directories live next to the executable; otherwise they fall back to a
/// platform user-data directory (via the [`directories`] crate):
///
/// | OS | Fallback `data_dir` |
/// |---|---|
/// | Windows | `%APPDATA%\dicom-viewer\` |
/// | macOS | `~/Library/Application Support/dicom-viewer/` |
/// | Linux | `~/.local/share/dicom-viewer/` |
#[derive(Debug, Clone)]
pub struct Paths {
    /// Directory containing the running executable.
    pub exe_dir: PathBuf,
    /// Where `config.toml`, `annotations.json`, and `logs/` are stored.
    pub data_dir: PathBuf,
    /// `data_dir/logs/`. Created on resolve.
    pub logs_dir: PathBuf,
    portable: bool,
}

impl Paths {
    /// Resolve paths for the current process.
    ///
    /// Tries to write a probe file into the exe directory; success means
    /// portable mode (the disc/USB is writable), failure means we fall
    /// back to the per-user data directory. Either way the `logs/`
    /// subdirectory is created if missing.
    ///
    /// # Errors
    /// Returns an error when [`std::env::current_exe`] fails or when the
    /// platform user-data directory can't be resolved (only possible on
    /// extremely unusual configurations).
    pub fn resolve() -> Result<Self> {
        let exe = std::env::current_exe().context("locating current exe")?;
        let exe_dir = exe
            .parent()
            .context("exe has no parent directory")?
            .to_path_buf();

        let (data_dir, portable) = if dir_is_writable(&exe_dir) {
            (exe_dir.clone(), true)
        } else {
            let proj = directories::ProjectDirs::from("dev", "AIMedic", "dicom-viewer")
                .context("could not resolve user data directory")?;
            (proj.data_dir().to_path_buf(), false)
        };

        let logs_dir = data_dir.join("logs");
        fs::create_dir_all(&logs_dir).ok();

        Ok(Self {
            exe_dir,
            data_dir,
            logs_dir,
            portable,
        })
    }

    /// `true` if the data directory lives next to the executable
    /// (writable exe dir), `false` if the user-data fallback is in use.
    pub fn is_portable(&self) -> bool {
        self.portable
    }

    /// Full path to `config.toml` (relative to [`Self::data_dir`]).
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join(CONFIG_FILE)
    }
}

fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(".dicom-viewer-write-probe");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// On-disk user settings. Serialised to `config.toml` next to the
/// executable (portable) or in the per-user data directory.
///
/// All fields default cleanly, so a missing or partial `config.toml` is
/// not an error — see the `#[serde(default)]` on each subsection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Remembered initial window size.
    pub window: WindowConfig,
    /// Theme + visible-panel state.
    pub ui: UiConfig,
    /// Whether the user has acknowledged the "Not for diagnostic use"
    /// disclaimer. Once `true`, the disclaimer never reappears on this
    /// machine.
    pub disclaimer_acknowledged: bool,
}

/// Initial viewer window dimensions in logical pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

/// Persisted UI preferences (theme + panel visibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Currently the theme is dark-only; this field is reserved.
    pub dark_mode: bool,
    /// Show the right-hand metadata panel on startup.
    pub show_metadata_panel: bool,
    /// Show the left-hand study browser on startup.
    pub show_study_browser: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            dark_mode: true,
            show_metadata_panel: false,
            show_study_browser: true,
        }
    }
}

impl Config {
    /// Read `config.toml` from `paths.config_path()`. Returns
    /// [`Config::default`] when the file is absent (first run).
    ///
    /// # Errors
    /// Returns an error only when the file exists but cannot be read or
    /// parsed as TOML.
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        // nosemgrep: path-traversal — `path` is composed from `paths.data_dir`,
        // which is derived from `std::env::current_exe()` or the platform
        // user-data dir (see `Paths::resolve`). There is no untrusted input
        // along this code path; this is a desktop binary, not an HTTP handler.
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    /// Write `self` as pretty TOML to `paths.config_path()`, creating the
    /// parent directory if needed. Called on app exit.
    ///
    /// # Errors
    /// Filesystem or serialisation failures.
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let path = paths.config_path();
        let text = toml::to_string_pretty(self).context("serializing config")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

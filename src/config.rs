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

#[derive(Debug, Clone)]
pub struct Paths {
    pub exe_dir: PathBuf,
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    portable: bool,
}

impl Paths {
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

    pub fn is_portable(&self) -> bool {
        self.portable
    }

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub disclaimer_acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub dark_mode: bool,
    pub show_metadata_panel: bool,
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

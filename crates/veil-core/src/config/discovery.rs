//! Config file discovery and loading — platform-aware path resolution.

use std::path::{Path, PathBuf};

use tracing::warn;

use super::parse::{parse_config, validate_config};
use super::{AppConfig, ConfigError};

/// Search for the config file in platform-specific locations.
/// Returns the path to the first config file found, or None if
/// no config file exists (the app should use defaults).
///
/// Search order:
/// 1. `~/.config/veil/config.toml` (all platforms)
/// 2. macOS: `~/Library/Application Support/com.veil.app/config.toml`
/// 3. Windows: `%APPDATA%\veil\config.toml`
pub fn discover_config_path() -> Option<PathBuf> {
    // 1. Check ~/.config/veil/config.toml (all platforms)
    if let Some(home) = dirs::home_dir() {
        let primary = home.join(".config").join("veil").join("config.toml");
        if primary.is_file() {
            return Some(primary);
        }
    }

    // 2. macOS: ~/Library/Application Support/com.veil.app/config.toml
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let macos_alt = home
                .join("Library")
                .join("Application Support")
                .join("com.veil.app")
                .join("config.toml");
            if macos_alt.is_file() {
                return Some(macos_alt);
            }
        }
    }

    // 3. Windows: %APPDATA%\veil\config.toml
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = dirs::config_dir() {
            let win_path = appdata.join("veil").join("config.toml");
            if win_path.is_file() {
                return Some(win_path);
            }
        }
    }

    None
}

/// Return the primary config directory path (for creating defaults).
/// `~/.config/veil/` on all platforms.
pub fn primary_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".config").join("veil"))
}

/// Load and parse a config file from the given path.
/// Returns the parsed config or a descriptive error.
pub fn load_config(path: &Path) -> Result<AppConfig, ConfigError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::ReadError { path: path.to_path_buf(), source: e })?;
    parse_config(&contents, path)
}

/// Load config from the discovered path, or return defaults if no file exists.
/// Returns (config, `Option<path>`) -- the path is None if using defaults.
pub fn load_or_default() -> (AppConfig, Option<PathBuf>) {
    match discover_config_path() {
        Some(path) => match load_config(&path) {
            Ok(config) => {
                let (validated, warnings) = validate_config(config);
                for w in &warnings {
                    warn!("{}: {}", w.field, w.message);
                }
                (validated, Some(path))
            }
            Err(e) => {
                warn!("failed to load config from {}: {e}, using defaults", path.display());
                (AppConfig::default(), Some(path))
            }
        },
        None => (AppConfig::default(), None),
    }
}

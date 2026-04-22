//! Configuration system — TOML config file parsing, validation, diffing, and hot-reload.
//!
//! The config system provides:
//! - Data model (`AppConfig` and sub-structs) with serde deserialization and sensible defaults
//! - Platform-aware config file discovery
//! - Rich TOML error reporting with line/column information
//! - Semantic validation with clamping and warnings
//! - Config diffing to determine what changed between two versions
//! - File watcher for hot-reload via the `notify` crate

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lifecycle::ShutdownHandle;

// ============================================================
// Unit 1: Config data model and defaults
// ============================================================

/// Top-level application configuration, deserialized from config.toml.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// General application settings.
    pub general: GeneralConfig,
    /// Sidebar settings.
    pub sidebar: SidebarConfig,
    /// Terminal settings.
    pub terminal: TerminalConfig,
    /// Conversations/adapters settings.
    pub conversations: ConversationsConfig,
    /// Keybinding settings.
    pub keybindings: KeybindingsConfig,
    /// Ghostty integration settings.
    pub ghostty: GhosttyConfig,
}

/// Theme preference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Dark theme.
    #[default]
    Dark,
    /// Light theme.
    Light,
    /// Follow system setting.
    System,
}

/// Workspace persistence behavior on exit/launch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistenceMode {
    /// Restore previous session.
    Restore,
    /// Start fresh.
    Fresh,
    /// Ask the user.
    #[default]
    Ask,
}

/// `[general]` section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Theme preference.
    pub theme: ThemeMode,
    /// Persistence behavior.
    pub persistence: PersistenceMode,
}

/// Which sidebar tab to show by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultTab {
    /// Show workspaces tab.
    #[default]
    Workspaces,
    /// Show conversations tab.
    Conversations,
}

/// `[sidebar]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarConfig {
    /// Default tab to show.
    pub default_tab: DefaultTab,
    /// Width in pixels.
    pub width: u32,
    /// Whether sidebar is visible on startup.
    pub visible: bool,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self { default_tab: DefaultTab::default(), width: 250, visible: true }
    }
}

/// Font weight specification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    /// 100 weight.
    Thin,
    /// 200 weight.
    ExtraLight,
    /// 300 weight.
    Light,
    /// 400 weight.
    #[default]
    Regular,
    /// 500 weight.
    Medium,
    /// 600 weight.
    SemiBold,
    /// 700 weight.
    Bold,
    /// 800 weight.
    ExtraBold,
    /// 900 weight.
    Black,
}

/// `[terminal]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    /// Number of lines in the scrollback buffer.
    pub scrollback_lines: u32,
    /// Font family name, or None for system default.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size: f32,
    /// Font weight.
    pub font_weight: FontWeight,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            font_family: None,
            font_size: 14.0,
            font_weight: FontWeight::default(),
        }
    }
}

/// `[conversations]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationsConfig {
    /// List of adapter names to enable.
    pub adapters: Vec<String>,
}

impl Default for ConversationsConfig {
    fn default() -> Self {
        Self { adapters: vec!["claude-code".to_string()] }
    }
}

/// `[keybindings]` section.
/// Keys are action names, values are shortcut strings.
/// Unknown keys are ignored with a warning.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Toggle sidebar visibility.
    pub toggle_sidebar: Option<String>,
    /// Switch to workspace tab.
    pub workspace_tab: Option<String>,
    /// Switch to conversations tab.
    pub conversations_tab: Option<String>,
    /// Create new workspace.
    pub new_workspace: Option<String>,
    /// Close current workspace.
    pub close_workspace: Option<String>,
    /// Split pane horizontally.
    pub split_horizontal: Option<String>,
    /// Split pane vertically.
    pub split_vertical: Option<String>,
    /// Close current pane.
    pub close_pane: Option<String>,
    /// Focus next pane.
    pub focus_next_pane: Option<String>,
    /// Focus previous pane.
    pub focus_previous_pane: Option<String>,
    /// Toggle pane zoom.
    pub zoom_pane: Option<String>,
    /// Focus pane to the left.
    pub focus_pane_left: Option<String>,
    /// Focus pane to the right.
    pub focus_pane_right: Option<String>,
    /// Focus pane above.
    pub focus_pane_up: Option<String>,
    /// Focus pane below.
    pub focus_pane_down: Option<String>,
}

/// `[ghostty]` section.
/// Placeholder for Ghostty config import (VEI-35).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhosttyConfig {
    /// Path to Ghostty config file.
    pub config_path: Option<String>,
}

// ============================================================
// Unit 2: Config file discovery
// ============================================================

/// Errors related to config file operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read a config file.
    #[error("failed to read config file at {path}: {source}")]
    ReadError {
        /// Path to the file that could not be read.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },

    /// Failed to parse a config file.
    #[error("failed to parse config at {path}: {message}")]
    ParseError {
        /// Path to the file that could not be parsed.
        path: PathBuf,
        /// Human-readable error message.
        message: String,
    },

    /// Failed to parse with known line/column.
    #[error("config error at {path} line {line}, column {column}: {message}")]
    ParseErrorWithLocation {
        /// Path to the file.
        path: PathBuf,
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        column: usize,
        /// Human-readable error message.
        message: String,
    },

    /// Could not determine config directory.
    #[error("failed to determine config directory")]
    NoConfigDir,
}

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
            Ok(config) => (config, Some(path)),
            Err(_) => (AppConfig::default(), Some(path)),
        },
        None => (AppConfig::default(), None),
    }
}

// ============================================================
// Unit 3: TOML parsing with rich error reporting
// ============================================================

/// A non-fatal issue found during config validation.
#[derive(Debug, Clone)]
pub struct ConfigWarning {
    /// The field path that triggered the warning (e.g., "sidebar.width").
    pub field: String,
    /// Human-readable warning message.
    pub message: String,
}

/// Parse a TOML string into `AppConfig` with rich error reporting.
/// On failure, returns a `ConfigError` with line/column and a
/// human-readable message describing the problem and suggesting fixes.
pub fn parse_config(toml_str: &str, source_path: &Path) -> Result<AppConfig, ConfigError> {
    toml::from_str(toml_str).map_err(|e| {
        let message = e.message().to_string();
        match e.span() {
            Some(span) => {
                // Calculate line and column from the byte offset
                let start = span.start;
                let mut line = 1;
                let mut col = 1;
                for (i, ch) in toml_str.char_indices() {
                    if i >= start {
                        break;
                    }
                    if ch == '\n' {
                        line += 1;
                        col = 1;
                    } else {
                        col += 1;
                    }
                }
                ConfigError::ParseErrorWithLocation {
                    path: source_path.to_path_buf(),
                    line,
                    column: col,
                    message,
                }
            }
            None => ConfigError::ParseError { path: source_path.to_path_buf(), message },
        }
    })
}

/// Validate a parsed config for semantic correctness.
///
/// Catches issues that are syntactically valid TOML but semantically wrong:
/// - `sidebar.width` < 100 or > 1000 (warning, clamp to range)
/// - `terminal.font_size` < 6.0 or > 72.0 (warning, clamp to range)
/// - `terminal.scrollback_lines` == 0 (warning, set to 1)
///
/// Returns the validated config and a list of warnings.
pub fn validate_config(config: AppConfig) -> (AppConfig, Vec<ConfigWarning>) {
    let mut config = config;
    let mut warnings = Vec::new();

    // sidebar.width: clamp to 100..=1000
    if config.sidebar.width < 100 {
        warnings.push(ConfigWarning {
            field: "sidebar.width".to_string(),
            message: format!(
                "sidebar.width {} is below minimum 100, clamped to 100",
                config.sidebar.width
            ),
        });
        config.sidebar.width = 100;
    } else if config.sidebar.width > 1000 {
        warnings.push(ConfigWarning {
            field: "sidebar.width".to_string(),
            message: format!(
                "sidebar.width {} is above maximum 1000, clamped to 1000",
                config.sidebar.width
            ),
        });
        config.sidebar.width = 1000;
    }

    // terminal.font_size: clamp to 6.0..=72.0
    if config.terminal.font_size < 6.0 {
        warnings.push(ConfigWarning {
            field: "terminal.font_size".to_string(),
            message: format!(
                "terminal.font_size {} is below minimum 6.0, clamped to 6.0",
                config.terminal.font_size
            ),
        });
        config.terminal.font_size = 6.0;
    } else if config.terminal.font_size > 72.0 {
        warnings.push(ConfigWarning {
            field: "terminal.font_size".to_string(),
            message: format!(
                "terminal.font_size {} is above maximum 72.0, clamped to 72.0",
                config.terminal.font_size
            ),
        });
        config.terminal.font_size = 72.0;
    }

    // terminal.scrollback_lines: if 0, set to 1
    if config.terminal.scrollback_lines == 0 {
        warnings.push(ConfigWarning {
            field: "terminal.scrollback_lines".to_string(),
            message: "terminal.scrollback_lines is 0, set to 1".to_string(),
        });
        config.terminal.scrollback_lines = 1;
    }

    (config, warnings)
}

// ============================================================
// Unit 4: Config diffing
// ============================================================

/// Describes what changed between two config versions.
/// Each field is `true` if that aspect of the config changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ConfigDelta {
    /// Theme setting changed.
    pub theme_changed: bool,
    /// Persistence setting changed.
    pub persistence_changed: bool,
    /// Sidebar settings changed.
    pub sidebar_changed: bool,
    /// Font settings changed.
    pub font_changed: bool,
    /// Scrollback setting changed.
    pub scrollback_changed: bool,
    /// Keybinding settings changed.
    pub keybindings_changed: bool,
    /// Adapter list changed.
    pub adapters_changed: bool,
    /// Ghostty config path changed.
    pub ghostty_path_changed: bool,
}

impl ConfigDelta {
    /// Returns true if nothing changed.
    pub fn is_empty(&self) -> bool {
        !self.theme_changed
            && !self.persistence_changed
            && !self.sidebar_changed
            && !self.font_changed
            && !self.scrollback_changed
            && !self.keybindings_changed
            && !self.adapters_changed
            && !self.ghostty_path_changed
    }

    /// Compute the delta between an old and new config.
    pub fn diff(old: &AppConfig, new: &AppConfig) -> Self {
        Self {
            theme_changed: old.general.theme != new.general.theme,
            persistence_changed: old.general.persistence != new.general.persistence,
            sidebar_changed: old.sidebar != new.sidebar,
            font_changed: old.terminal.font_family != new.terminal.font_family
                || (old.terminal.font_size - new.terminal.font_size).abs() > f32::EPSILON
                || old.terminal.font_weight != new.terminal.font_weight,
            scrollback_changed: old.terminal.scrollback_lines != new.terminal.scrollback_lines,
            keybindings_changed: old.keybindings != new.keybindings,
            adapters_changed: old.conversations != new.conversations,
            ghostty_path_changed: old.ghostty != new.ghostty,
        }
    }
}

// ============================================================
// Unit 5: Config file watcher (hot-reload)
// ============================================================

/// Event emitted by the config watcher.
#[derive(Debug)]
pub enum ConfigEvent {
    /// Config was successfully reloaded.
    Reloaded {
        /// The new config.
        config: Box<AppConfig>,
        /// What changed.
        delta: ConfigDelta,
        /// Non-fatal validation warnings.
        warnings: Vec<ConfigWarning>,
    },
    /// Config file was modified but had errors; previous config retained.
    Error(ConfigError),
}

/// Watches the config file for changes and emits `ConfigEvent`s.
pub struct ConfigWatcher {
    config_path: PathBuf,
    current_config: AppConfig,
    #[allow(dead_code)]
    event_tx: tokio::sync::mpsc::Sender<ConfigEvent>,
}

impl ConfigWatcher {
    /// Create a new watcher for the given config file path.
    /// `event_tx` is a channel sender for delivering config events.
    /// The watcher does NOT start until `start()` is called.
    pub fn new(
        config_path: PathBuf,
        initial_config: AppConfig,
        event_tx: tokio::sync::mpsc::Sender<ConfigEvent>,
    ) -> Result<Self, ConfigError> {
        Ok(Self { config_path, current_config: initial_config, event_tx })
    }

    /// Start watching for file changes.
    /// This spawns an internal task that runs until the watcher is dropped
    /// or a shutdown signal is received.
    pub fn start(&mut self, _shutdown: ShutdownHandle) -> Result<(), ConfigError> {
        // STUB: does nothing
        Ok(())
    }

    /// Get the currently active (valid) config.
    pub fn current_config(&self) -> &AppConfig {
        &self.current_config
    }

    /// Manually trigger a reload (useful for testing or user-initiated reload).
    pub fn reload(&mut self) -> Result<ConfigEvent, ConfigError> {
        // STUB: always returns error
        Err(ConfigError::ReadError {
            path: self.config_path.clone(),
            source: std::io::Error::other("stub: not implemented"),
        })
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ============================================================
    // Unit 1: Config data model and defaults
    // ============================================================

    mod unit1_defaults {
        use super::*;

        #[test]
        fn theme_mode_default_is_dark() {
            assert_eq!(ThemeMode::default(), ThemeMode::Dark);
        }

        #[test]
        fn persistence_mode_default_is_ask() {
            assert_eq!(PersistenceMode::default(), PersistenceMode::Ask);
        }

        #[test]
        fn default_tab_default_is_workspaces() {
            assert_eq!(DefaultTab::default(), DefaultTab::Workspaces);
        }

        #[test]
        fn font_weight_default_is_regular() {
            assert_eq!(FontWeight::default(), FontWeight::Regular);
        }

        #[test]
        fn general_config_defaults() {
            let general = GeneralConfig::default();
            assert_eq!(general.theme, ThemeMode::Dark);
            assert_eq!(general.persistence, PersistenceMode::Ask);
        }

        #[test]
        fn sidebar_config_defaults() {
            let sidebar = SidebarConfig::default();
            assert_eq!(sidebar.default_tab, DefaultTab::Workspaces);
            assert_eq!(sidebar.width, 250);
            assert!(sidebar.visible);
        }

        #[test]
        fn terminal_config_defaults() {
            let terminal = TerminalConfig::default();
            assert_eq!(terminal.scrollback_lines, 10000);
            assert!(terminal.font_family.is_none());
            assert!((terminal.font_size - 14.0).abs() < f32::EPSILON);
            assert_eq!(terminal.font_weight, FontWeight::Regular);
        }

        #[test]
        fn conversations_config_defaults() {
            let conversations = ConversationsConfig::default();
            assert_eq!(conversations.adapters, vec!["claude-code".to_string()]);
        }

        #[test]
        fn keybindings_config_defaults_all_none() {
            let kb = KeybindingsConfig::default();
            assert!(kb.toggle_sidebar.is_none());
            assert!(kb.workspace_tab.is_none());
            assert!(kb.conversations_tab.is_none());
            assert!(kb.new_workspace.is_none());
            assert!(kb.close_workspace.is_none());
            assert!(kb.split_horizontal.is_none());
            assert!(kb.split_vertical.is_none());
            assert!(kb.close_pane.is_none());
            assert!(kb.focus_next_pane.is_none());
            assert!(kb.focus_previous_pane.is_none());
            assert!(kb.zoom_pane.is_none());
            assert!(kb.focus_pane_left.is_none());
            assert!(kb.focus_pane_right.is_none());
            assert!(kb.focus_pane_up.is_none());
            assert!(kb.focus_pane_down.is_none());
        }

        #[test]
        fn ghostty_config_defaults() {
            let ghostty = GhosttyConfig::default();
            assert!(ghostty.config_path.is_none());
        }

        #[test]
        fn app_config_default_has_correct_general() {
            let config = AppConfig::default();
            assert_eq!(config.general.theme, ThemeMode::Dark);
            assert_eq!(config.general.persistence, PersistenceMode::Ask);
        }

        #[test]
        fn app_config_default_has_correct_sidebar() {
            let config = AppConfig::default();
            assert_eq!(config.sidebar.default_tab, DefaultTab::Workspaces);
            assert_eq!(config.sidebar.width, 250);
            assert!(config.sidebar.visible);
        }

        #[test]
        fn app_config_default_has_correct_terminal() {
            let config = AppConfig::default();
            assert_eq!(config.terminal.scrollback_lines, 10000);
            assert!(config.terminal.font_family.is_none());
            assert!((config.terminal.font_size - 14.0).abs() < f32::EPSILON);
            assert_eq!(config.terminal.font_weight, FontWeight::Regular);
        }

        #[test]
        fn app_config_default_has_correct_conversations() {
            let config = AppConfig::default();
            assert_eq!(config.conversations.adapters, vec!["claude-code".to_string()]);
        }

        #[test]
        fn app_config_default_has_correct_keybindings() {
            let config = AppConfig::default();
            assert!(config.keybindings.toggle_sidebar.is_none());
        }

        #[test]
        fn app_config_default_has_correct_ghostty() {
            let config = AppConfig::default();
            assert!(config.ghostty.config_path.is_none());
        }
    }

    mod unit1_serde {
        use super::*;

        #[test]
        fn empty_toml_deserializes_to_defaults() {
            let config: AppConfig = toml::from_str("").expect("empty string should parse");
            let expected = AppConfig::default();
            assert_eq!(config, expected);
        }

        #[test]
        fn partial_toml_only_general_fills_defaults() {
            let toml_str = r#"
[general]
theme = "dark"
"#;
            let config: AppConfig = toml::from_str(toml_str).expect("partial TOML should parse");
            // Sidebar, terminal, etc. should all be default
            assert_eq!(config.sidebar, SidebarConfig::default());
            assert_eq!(config.terminal, TerminalConfig::default());
            assert_eq!(config.conversations, ConversationsConfig::default());
        }

        #[test]
        fn partial_section_fills_missing_fields() {
            let toml_str = r#"
[general]
theme = "light"
"#;
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert_eq!(config.general.theme, ThemeMode::Light);
            // persistence should be default
            assert_eq!(config.general.persistence, PersistenceMode::default());
        }

        // TOML requires enums to be inside a struct for serialization.
        // We use a full AppConfig round-trip to test enum serde.

        #[test]
        fn theme_mode_round_trip_dark() {
            let toml_str = "[general]\ntheme = \"dark\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.theme, ThemeMode::Dark);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.theme, ThemeMode::Dark);
        }

        #[test]
        fn theme_mode_round_trip_light() {
            let toml_str = "[general]\ntheme = \"light\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.theme, ThemeMode::Light);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.theme, ThemeMode::Light);
        }

        #[test]
        fn theme_mode_round_trip_system() {
            let toml_str = "[general]\ntheme = \"system\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.theme, ThemeMode::System);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.theme, ThemeMode::System);
        }

        #[test]
        fn persistence_mode_round_trip_restore() {
            let toml_str = "[general]\npersistence = \"restore\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.persistence, PersistenceMode::Restore);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.persistence, PersistenceMode::Restore);
        }

        #[test]
        fn persistence_mode_round_trip_fresh() {
            let toml_str = "[general]\npersistence = \"fresh\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.persistence, PersistenceMode::Fresh);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.persistence, PersistenceMode::Fresh);
        }

        #[test]
        fn persistence_mode_round_trip_ask() {
            let toml_str = "[general]\npersistence = \"ask\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.general.persistence, PersistenceMode::Ask);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.general.persistence, PersistenceMode::Ask);
        }

        #[test]
        fn default_tab_round_trip_workspaces() {
            let toml_str = "[sidebar]\ndefault_tab = \"workspaces\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.sidebar.default_tab, DefaultTab::Workspaces);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.sidebar.default_tab, DefaultTab::Workspaces);
        }

        #[test]
        fn default_tab_round_trip_conversations() {
            let toml_str = "[sidebar]\ndefault_tab = \"conversations\"\n";
            let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
            assert_eq!(config.sidebar.default_tab, DefaultTab::Conversations);
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
            assert_eq!(roundtrip.sidebar.default_tab, DefaultTab::Conversations);
        }

        #[test]
        fn font_weight_round_trip_all_variants() {
            let variant_strs = [
                ("thin", FontWeight::Thin),
                ("extralight", FontWeight::ExtraLight),
                ("light", FontWeight::Light),
                ("regular", FontWeight::Regular),
                ("medium", FontWeight::Medium),
                ("semibold", FontWeight::SemiBold),
                ("bold", FontWeight::Bold),
                ("extrabold", FontWeight::ExtraBold),
                ("black", FontWeight::Black),
            ];
            for (name, expected) in &variant_strs {
                let toml_str = format!("[terminal]\nfont_weight = \"{name}\"\n");
                let config: AppConfig = toml::from_str(&toml_str).expect("deserialize");
                assert_eq!(
                    &config.terminal.font_weight, expected,
                    "deserialization failed for {name}"
                );
                let serialized = toml::to_string(&config).expect("serialize");
                let roundtrip: AppConfig = toml::from_str(&serialized).expect("round-trip");
                assert_eq!(
                    &roundtrip.terminal.font_weight, expected,
                    "round-trip failed for {name}"
                );
            }
        }

        #[test]
        fn font_size_accepts_integer_value() {
            let toml_str = r"
[terminal]
font_size = 14
";
            let config: AppConfig =
                toml::from_str(toml_str).expect("integer font_size should parse");
            assert!((config.terminal.font_size - 14.0).abs() < f32::EPSILON);
        }

        #[test]
        fn adapters_list_preserves_order() {
            let toml_str = r#"
[conversations]
adapters = ["claude-code", "codex", "opencode"]
"#;
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert_eq!(config.conversations.adapters, vec!["claude-code", "codex", "opencode"]);
        }

        #[test]
        fn keybindings_partial_fields() {
            let toml_str = r#"
[keybindings]
workspace_tab = "ctrl+shift+w"
"#;
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert_eq!(config.keybindings.workspace_tab, Some("ctrl+shift+w".to_string()));
            assert!(config.keybindings.toggle_sidebar.is_none());
            assert!(config.keybindings.conversations_tab.is_none());
        }

        #[test]
        fn ghostty_config_path_none() {
            let toml_str = r"
[ghostty]
";
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert!(config.ghostty.config_path.is_none());
        }

        #[test]
        fn ghostty_config_path_with_value() {
            let toml_str = r#"
[ghostty]
config_path = "/home/user/.config/ghostty/config"
"#;
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert_eq!(
                config.ghostty.config_path,
                Some("/home/user/.config/ghostty/config".to_string())
            );
        }
    }

    mod unit1_equality {
        use super::*;

        #[test]
        fn same_configs_are_equal() {
            let a = AppConfig::default();
            let b = AppConfig::default();
            assert_eq!(a, b);
        }

        #[test]
        fn different_configs_are_not_equal() {
            let a = AppConfig::default();
            let mut b = AppConfig::default();
            b.general.theme = ThemeMode::System;
            assert_ne!(a, b);
        }
    }

    // ============================================================
    // Unit 2: Config file discovery
    // ============================================================

    mod unit2_discovery {
        use super::*;

        #[test]
        fn primary_config_dir_returns_path_ending_in_veil() {
            let dir = primary_config_dir();
            assert!(dir.is_some(), "primary_config_dir should return Some");
            let path = dir.unwrap();
            assert!(path.ends_with("veil"), "config dir should end with 'veil', got: {path:?}");
        }

        #[test]
        fn discover_config_path_returns_none_when_no_files_exist() {
            // When no config files exist at any search location, should return None.
            // This test uses the real filesystem — if the user has a real config file,
            // this may pass anyway, but that's fine since this is a RED test.
            let result = discover_config_path();
            // We can't assert None because the user might have a real config file.
            // Instead, this test verifies the function doesn't panic.
            let _ = result;
        }
    }

    mod unit2_loading {
        use super::*;

        #[test]
        fn load_config_with_valid_toml() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "dark"
persistence = "ask"

[sidebar]
width = 300
"#,
            )
            .expect("write config");

            let result = load_config(&config_path);
            assert!(result.is_ok(), "load_config with valid TOML should succeed");
            let config = result.unwrap();
            assert_eq!(config.general.theme, ThemeMode::Dark);
            assert_eq!(config.sidebar.width, 300);
        }

        #[test]
        fn load_config_with_invalid_toml_returns_parse_error() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "invalid = [toml that is broken").expect("write");

            let result = load_config(&config_path);
            assert!(result.is_err(), "invalid TOML should produce an error");
            let err = result.unwrap_err();
            match err {
                ConfigError::ParseError { path, .. }
                | ConfigError::ParseErrorWithLocation { path, .. } => {
                    assert_eq!(path, config_path);
                }
                other => panic!("expected ParseError, got: {other}"),
            }
        }

        #[test]
        fn load_config_with_nonexistent_path_returns_read_error() {
            let path = PathBuf::from("/tmp/veil-test-nonexistent-config-file.toml");
            let result = load_config(&path);
            assert!(result.is_err());
            match result.unwrap_err() {
                ConfigError::ReadError { path: err_path, .. } => {
                    assert_eq!(err_path, path);
                }
                other => panic!("expected ReadError, got: {other}"),
            }
        }

        #[test]
        fn load_config_with_empty_file_returns_defaults() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write");

            let result = load_config(&config_path);
            assert!(result.is_ok(), "empty file should parse to defaults");
            let config = result.unwrap();
            assert_eq!(config, AppConfig::default());
        }

        #[test]
        fn load_config_with_partial_toml_fills_defaults() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("write");

            let result = load_config(&config_path);
            assert!(result.is_ok(), "partial TOML should succeed");
            let config = result.unwrap();
            assert_eq!(config.general.theme, ThemeMode::Light);
            // Rest should be defaults
            assert_eq!(config.sidebar, SidebarConfig::default());
        }

        #[test]
        fn load_or_default_returns_defaults_when_no_file() {
            let (config, path) = load_or_default();
            // Even if this returns defaults + None, we verify the config is the default
            assert_eq!(config, AppConfig::default());
            // path should be None when no config file exists
            // (This may not hold if user has a real config, but tests the stub)
            assert!(path.is_none());
        }

        #[test]
        fn parse_error_includes_file_path() {
            let err = ConfigError::ParseError {
                path: PathBuf::from("/home/user/.config/veil/config.toml"),
                message: "unexpected character".to_string(),
            };
            let display = format!("{err}");
            assert!(
                display.contains("/home/user/.config/veil/config.toml"),
                "error should contain the file path"
            );
            assert!(display.contains("unexpected character"), "error should contain the message");
        }

        #[test]
        fn parse_error_with_location_includes_line_and_column() {
            let err = ConfigError::ParseErrorWithLocation {
                path: PathBuf::from("/config.toml"),
                line: 5,
                column: 12,
                message: "invalid type".to_string(),
            };
            let display = format!("{err}");
            assert!(display.contains("line 5"), "should contain line number");
            assert!(display.contains("column 12"), "should contain column number");
            assert!(display.contains("invalid type"), "should contain message");
            assert!(display.contains("/config.toml"), "should contain path");
        }

        #[test]
        fn read_error_includes_path_and_io_error() {
            let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
            let err = ConfigError::ReadError {
                path: PathBuf::from("/etc/veil/config.toml"),
                source: io_err,
            };
            let display = format!("{err}");
            assert!(display.contains("/etc/veil/config.toml"), "should contain path");
            assert!(display.contains("access denied"), "should contain IO error");
        }

        #[test]
        fn no_config_dir_error_is_informative() {
            let err = ConfigError::NoConfigDir;
            let display = format!("{err}");
            assert!(
                display.contains("config directory"),
                "NoConfigDir should mention config directory"
            );
        }

        #[test]
        fn toml_parse_error_includes_line_column_when_available() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            // Write TOML with a syntax error on a specific line
            std::fs::write(&config_path, "# line 1\n# line 2\n[general\ntheme = \"dark\"\n")
                .expect("write");

            let result = load_config(&config_path);
            assert!(result.is_err());
            let err = result.unwrap_err();
            // The error should contain location info
            let display = format!("{err}");
            assert!(
                display.contains("line") || display.contains('3'),
                "error should contain line info: {display}"
            );
        }
    }

    // ============================================================
    // Unit 3: TOML parsing with rich error reporting
    // ============================================================

    mod unit3_parsing {
        use super::*;

        #[test]
        fn full_valid_config_parses_all_sections() {
            let toml_str = r#"
[general]
theme = "dark"
persistence = "restore"

[sidebar]
default_tab = "conversations"
width = 300
visible = false

[terminal]
scrollback_lines = 20000
font_family = "JetBrains Mono"
font_size = 16.0
font_weight = "medium"

[conversations]
adapters = ["claude-code", "codex"]

[keybindings]
toggle_sidebar = "ctrl+b"
workspace_tab = "ctrl+shift+w"

[ghostty]
config_path = "/home/user/.config/ghostty/config"
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_ok(), "full valid config should parse: {result:?}");

            let config = result.unwrap();
            assert_eq!(config.general.theme, ThemeMode::Dark);
            assert_eq!(config.general.persistence, PersistenceMode::Restore);
            assert_eq!(config.sidebar.default_tab, DefaultTab::Conversations);
            assert_eq!(config.sidebar.width, 300);
            assert!(!config.sidebar.visible);
            assert_eq!(config.terminal.scrollback_lines, 20000);
            assert_eq!(config.terminal.font_family, Some("JetBrains Mono".to_string()));
            assert!((config.terminal.font_size - 16.0).abs() < f32::EPSILON);
            assert_eq!(config.terminal.font_weight, FontWeight::Medium);
            assert_eq!(config.conversations.adapters, vec!["claude-code", "codex"]);
            assert_eq!(config.keybindings.toggle_sidebar, Some("ctrl+b".to_string()));
            assert_eq!(config.keybindings.workspace_tab, Some("ctrl+shift+w".to_string()));
            assert_eq!(
                config.ghostty.config_path,
                Some("/home/user/.config/ghostty/config".to_string())
            );
        }

        #[test]
        fn minimal_config_single_field_parses() {
            let toml_str = r#"
[general]
theme = "light"
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_ok(), "minimal config should parse: {result:?}");
            let config = result.unwrap();
            assert_eq!(config.general.theme, ThemeMode::Light);
            // Everything else should be defaults
            assert_eq!(config.sidebar, SidebarConfig::default());
        }

        #[test]
        fn comments_in_toml_are_ignored() {
            let toml_str = r#"
# This is a comment
[general]
# Another comment
theme = "dark" # inline comment
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_ok(), "comments should be ignored: {result:?}");
        }

        #[test]
        fn inline_tables_work() {
            let toml_str = r#"
keybindings = { workspace_tab = "ctrl+shift+w" }
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_ok(), "inline tables should work: {result:?}");
            let config = result.unwrap();
            assert_eq!(config.keybindings.workspace_tab, Some("ctrl+shift+w".to_string()));
        }

        #[test]
        fn invalid_toml_syntax_produces_error_with_line() {
            let toml_str = "[general\ntheme = \"dark\"\n";
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_err(), "invalid syntax should produce error");
            let err = result.unwrap_err();
            let display = format!("{err}");
            assert!(display.contains("test.toml"), "should contain path");
        }

        #[test]
        fn wrong_type_for_field_produces_error() {
            let toml_str = r#"
[sidebar]
width = "not a number"
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_err(), "wrong type should produce error");
        }

        #[test]
        fn unknown_section_is_silently_ignored() {
            let toml_str = r#"
[nonexistent]
foo = "bar"

[general]
theme = "dark"
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            // This depends on serde's deny_unknown_fields vs default behavior.
            // By default, serde ignores unknown fields, so this should succeed.
            assert!(result.is_ok(), "unknown section should be ignored: {result:?}");
        }

        #[test]
        fn unknown_field_in_known_section_is_ignored() {
            let toml_str = r#"
[sidebar]
foo = "bar"
width = 300
"#;
            let path = PathBuf::from("test.toml");
            let result = parse_config(toml_str, &path);
            assert!(result.is_ok(), "unknown field should be ignored: {result:?}");
            let config = result.unwrap();
            assert_eq!(config.sidebar.width, 300);
        }
    }

    mod unit3_validation {
        use super::*;

        #[test]
        fn sidebar_width_below_100_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.sidebar.width = 50;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.sidebar.width, 100);
            assert!(
                warnings.iter().any(|w| w.field.contains("sidebar.width")),
                "should have a warning about sidebar.width"
            );
        }

        #[test]
        fn sidebar_width_above_1000_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.sidebar.width = 2000;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.sidebar.width, 1000);
            assert!(
                warnings.iter().any(|w| w.field.contains("sidebar.width")),
                "should have a warning about sidebar.width"
            );
        }

        #[test]
        fn font_size_below_6_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.font_size = 2.0;
            let (validated, warnings) = validate_config(config);
            assert!((validated.terminal.font_size - 6.0).abs() < f32::EPSILON);
            assert!(
                warnings.iter().any(|w| w.field.contains("terminal.font_size")),
                "should have a warning about font_size"
            );
        }

        #[test]
        fn font_size_above_72_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.font_size = 100.0;
            let (validated, warnings) = validate_config(config);
            assert!((validated.terminal.font_size - 72.0).abs() < f32::EPSILON);
            assert!(
                warnings.iter().any(|w| w.field.contains("terminal.font_size")),
                "should have a warning about font_size"
            );
        }

        #[test]
        fn scrollback_lines_zero_set_to_1_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.scrollback_lines = 0;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.terminal.scrollback_lines, 1);
            assert!(
                warnings.iter().any(|w| w.field.contains("terminal.scrollback_lines")),
                "should have a warning about scrollback_lines"
            );
        }

        #[test]
        fn valid_values_pass_without_warnings() {
            let mut config = AppConfig::default();
            config.sidebar.width = 300;
            config.terminal.font_size = 14.0;
            config.terminal.scrollback_lines = 10000;
            let (validated, warnings) = validate_config(config.clone());
            assert!(warnings.is_empty(), "valid config should have no warnings, got: {warnings:?}");
            assert_eq!(validated.sidebar.width, 300);
        }

        #[test]
        fn multiple_validation_issues_produce_multiple_warnings() {
            let mut config = AppConfig::default();
            config.sidebar.width = 50;
            config.terminal.font_size = 2.0;
            config.terminal.scrollback_lines = 0;
            let (_, warnings) = validate_config(config);
            assert!(warnings.len() >= 3, "should have at least 3 warnings, got {}", warnings.len());
        }
    }

    // ============================================================
    // Unit 4: Config diffing
    // ============================================================

    mod unit4_diffing {
        use super::*;

        #[test]
        fn diff_defaults_returns_empty_delta() {
            let a = AppConfig::default();
            let b = AppConfig::default();
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.is_empty(), "diff of two defaults should be empty");
        }

        #[test]
        fn diff_clone_returns_empty_delta() {
            let a = AppConfig::default();
            let b = a.clone();
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.is_empty(), "diff of clone should be empty");
        }

        #[test]
        fn is_empty_returns_true_on_empty_delta() {
            let delta = ConfigDelta::default();
            assert!(delta.is_empty());
        }

        #[test]
        fn changing_theme_sets_theme_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.general.theme = ThemeMode::System;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.theme_changed, "theme_changed should be true");
            assert!(!delta.persistence_changed);
            assert!(!delta.sidebar_changed);
            assert!(!delta.font_changed);
            assert!(!delta.scrollback_changed);
            assert!(!delta.keybindings_changed);
            assert!(!delta.adapters_changed);
            assert!(!delta.ghostty_path_changed);
        }

        #[test]
        fn changing_persistence_sets_persistence_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.general.persistence = PersistenceMode::Restore;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.persistence_changed, "persistence_changed should be true");
            assert!(!delta.theme_changed);
        }

        #[test]
        fn changing_sidebar_width_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.width = 400;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed, "sidebar_changed should be true");
            assert!(!delta.theme_changed);
        }

        #[test]
        fn changing_sidebar_visible_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.visible = !a.sidebar.visible;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed, "sidebar_changed should be true");
        }

        #[test]
        fn changing_sidebar_default_tab_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.default_tab = DefaultTab::Conversations;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed, "sidebar_changed should be true");
        }

        #[test]
        fn changing_font_family_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_family = Some("Fira Code".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed, "font_changed should be true");
            assert!(!delta.scrollback_changed);
        }

        #[test]
        fn changing_font_size_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_size = 18.0;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed, "font_changed should be true");
        }

        #[test]
        fn changing_font_weight_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_weight = FontWeight::Bold;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed, "font_changed should be true");
        }

        #[test]
        fn changing_scrollback_sets_scrollback_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.scrollback_lines = 50000;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.scrollback_changed, "scrollback_changed should be true");
            assert!(!delta.font_changed);
        }

        #[test]
        fn changing_keybindings_sets_keybindings_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.keybindings.toggle_sidebar = Some("ctrl+b".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.keybindings_changed, "keybindings_changed should be true");
        }

        #[test]
        fn changing_adapters_sets_adapters_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.conversations.adapters = vec!["codex".to_string()];
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.adapters_changed, "adapters_changed should be true");
        }

        #[test]
        fn changing_ghostty_path_sets_ghostty_path_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.ghostty.config_path = Some("/path/to/ghostty".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.ghostty_path_changed, "ghostty_path_changed should be true");
        }

        #[test]
        fn changing_theme_and_font_sets_both() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.general.theme = ThemeMode::System;
            b.terminal.font_size = 18.0;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.theme_changed);
            assert!(delta.font_changed);
            assert!(!delta.sidebar_changed);
            assert!(!delta.scrollback_changed);
        }

        #[test]
        fn is_empty_returns_false_when_any_field_true() {
            let delta = ConfigDelta { theme_changed: true, ..ConfigDelta::default() };
            assert!(!delta.is_empty(), "is_empty should be false when any field is true");
        }
    }

    // ============================================================
    // Unit 5: Config file watcher (hot-reload)
    // ============================================================

    mod unit5_watcher {
        use super::*;

        #[test]
        fn watcher_new_with_valid_path_succeeds() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let result = ConfigWatcher::new(config_path, AppConfig::default(), tx);
            assert!(result.is_ok(), "ConfigWatcher::new should succeed");
        }

        #[test]
        fn watcher_current_config_returns_initial() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let initial = AppConfig::default();
            let watcher =
                ConfigWatcher::new(config_path, initial.clone(), tx).expect("create watcher");
            assert_eq!(watcher.current_config(), &initial);
        }

        #[test]
        fn reload_after_valid_config_change_returns_reloaded() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            // Write a new valid config
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("write new config");

            let result = watcher.reload();
            assert!(result.is_ok(), "reload should succeed: {result:?}");
            match result.unwrap() {
                ConfigEvent::Reloaded { config, delta, .. } => {
                    assert_eq!(config.general.theme, ThemeMode::Light);
                    // Delta should show theme changed
                    assert!(delta.theme_changed);
                }
                ConfigEvent::Error(e) => panic!("expected Reloaded, got Error: {e}"),
            }
        }

        #[test]
        fn reload_after_invalid_toml_returns_error() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            // Write invalid TOML
            std::fs::write(&config_path, "[broken").expect("write invalid config");

            let result = watcher.reload();
            assert!(result.is_ok(), "reload should return Ok(ConfigEvent::Error)");
            match result.unwrap() {
                ConfigEvent::Error(_) => {} // expected
                ConfigEvent::Reloaded { .. } => panic!("expected Error event, got Reloaded"),
            }
        }

        #[test]
        fn reload_with_no_changes_returns_empty_delta() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let mut watcher =
                ConfigWatcher::new(config_path, AppConfig::default(), tx).expect("create watcher");

            let result = watcher.reload();
            assert!(result.is_ok(), "reload with no changes should succeed");
            match result.unwrap() {
                ConfigEvent::Reloaded { delta, .. } => {
                    assert!(delta.is_empty(), "delta should be empty when nothing changed");
                }
                ConfigEvent::Error(e) => panic!("expected Reloaded, got Error: {e}"),
            }
        }

        #[test]
        fn after_failed_reload_current_config_unchanged() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let initial = AppConfig::default();
            let mut watcher = ConfigWatcher::new(config_path.clone(), initial.clone(), tx)
                .expect("create watcher");

            // Write invalid TOML
            std::fs::write(&config_path, "[broken").expect("write invalid");
            let _ = watcher.reload();

            // Current config should still be the initial
            assert_eq!(
                watcher.current_config(),
                &initial,
                "config should be unchanged after failed reload"
            );
        }

        #[tokio::test]
        async fn file_watcher_receives_reloaded_on_modify() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            let signal = crate::lifecycle::ShutdownSignal::new();
            let handle = signal.handle();
            watcher.start(handle).expect("start watcher");

            // Modify the file
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("write new config");

            // Wait for event with timeout
            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;

            // Shut down
            signal.trigger();

            assert!(event.is_ok(), "should receive event within timeout");
            let event = event.unwrap();
            assert!(event.is_some(), "channel should not be closed");
            match event.unwrap() {
                ConfigEvent::Reloaded { config, .. } => {
                    assert_eq!(config.general.theme, ThemeMode::Light);
                }
                ConfigEvent::Error(e) => panic!("expected Reloaded, got Error: {e}"),
            }
        }

        #[tokio::test]
        async fn file_watcher_error_on_invalid_toml_retains_config() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            let signal = crate::lifecycle::ShutdownSignal::new();
            let handle = signal.handle();
            watcher.start(handle).expect("start watcher");

            // Write invalid TOML
            std::fs::write(&config_path, "[broken syntax").expect("write invalid");

            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;

            signal.trigger();

            assert!(event.is_ok(), "should receive event");
            let event = event.unwrap();
            assert!(event.is_some());
            match event.unwrap() {
                ConfigEvent::Error(_) => {} // expected
                ConfigEvent::Reloaded { .. } => {
                    panic!("expected Error, got Reloaded for invalid TOML")
                }
            }
        }

        #[tokio::test]
        async fn file_watcher_recovers_after_error() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            let signal = crate::lifecycle::ShutdownSignal::new();
            let handle = signal.handle();
            watcher.start(handle).expect("start watcher");

            // Write invalid TOML first
            std::fs::write(&config_path, "[broken").expect("write invalid");
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;

            // Now write valid TOML
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "system"
"#,
            )
            .expect("write valid");

            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;

            signal.trigger();

            assert!(event.is_ok(), "should receive recovery event");
            let event = event.unwrap();
            assert!(event.is_some());
            match event.unwrap() {
                ConfigEvent::Reloaded { config, .. } => {
                    assert_eq!(config.general.theme, ThemeMode::System);
                }
                ConfigEvent::Error(_) => {
                    // This is also acceptable if the watcher sends the error first
                }
            }
        }

        #[tokio::test]
        async fn file_watcher_delete_and_recreate_triggers_reload() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            let signal = crate::lifecycle::ShutdownSignal::new();
            let handle = signal.handle();
            watcher.start(handle).expect("start watcher");

            // Delete the file
            std::fs::remove_file(&config_path).expect("delete file");
            // Small delay to let filesystem notify
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            // Recreate with new content
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("recreate file");

            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;

            signal.trigger();

            assert!(event.is_ok(), "should receive event after recreate");
        }

        #[test]
        fn reload_delta_only_theme_changed() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            // Write a config that only changes theme
            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("write");

            let result = watcher.reload();
            assert!(result.is_ok());
            match result.unwrap() {
                ConfigEvent::Reloaded { delta, .. } => {
                    assert!(delta.theme_changed, "theme_changed should be true");
                    assert!(!delta.sidebar_changed, "sidebar_changed should be false");
                    assert!(!delta.font_changed, "font_changed should be false");
                }
                ConfigEvent::Error(e) => panic!("expected Reloaded, got Error: {e}"),
            }
        }

        #[test]
        fn reload_delta_font_and_sidebar_changed() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let mut watcher = ConfigWatcher::new(config_path.clone(), AppConfig::default(), tx)
                .expect("create watcher");

            std::fs::write(
                &config_path,
                r"
[sidebar]
width = 400

[terminal]
font_size = 18.0
",
            )
            .expect("write");

            let result = watcher.reload();
            assert!(result.is_ok());
            match result.unwrap() {
                ConfigEvent::Reloaded { delta, .. } => {
                    assert!(delta.sidebar_changed, "sidebar_changed should be true");
                    assert!(delta.font_changed, "font_changed should be true");
                    assert!(!delta.theme_changed, "theme_changed should be false");
                }
                ConfigEvent::Error(e) => panic!("expected Reloaded, got Error: {e}"),
            }
        }

        #[tokio::test]
        async fn watcher_stops_on_shutdown() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write initial");

            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            let mut watcher =
                ConfigWatcher::new(config_path, AppConfig::default(), tx).expect("create watcher");

            let signal = crate::lifecycle::ShutdownSignal::new();
            let handle = signal.handle();
            watcher.start(handle).expect("start watcher");

            // Trigger shutdown
            signal.trigger();

            // Give watcher time to stop
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            // After shutdown, the channel should be closed (no more senders)
            // This checks that the watcher dropped its sender
            let result = rx.try_recv();
            // Either Empty (no events) or Disconnected (channel closed) is acceptable
            assert!(result.is_err(), "after shutdown, no events should be pending");
        }
    }

    // ============================================================
    // Unit 6: StateUpdate::ConfigReloaded tests
    // (These test the new variant shape in message.rs)
    // ============================================================

    mod unit6_state_update {
        use super::*;
        use crate::message::{Channels, StateUpdate};

        #[test]
        fn config_reloaded_can_be_constructed_and_matched() {
            let config = AppConfig::default();
            let delta = ConfigDelta::default();
            let warnings = vec![ConfigWarning {
                field: "test".to_string(),
                message: "test warning".to_string(),
            }];

            let update = StateUpdate::ConfigReloaded {
                config: Box::new(config.clone()),
                delta: delta.clone(),
                warnings: warnings.clone(),
            };

            match update {
                StateUpdate::ConfigReloaded { config: c, delta: d, warnings: w } => {
                    assert_eq!(*c, config);
                    assert_eq!(d, delta);
                    assert_eq!(w.len(), 1);
                    assert_eq!(w[0].field, "test");
                }
                _ => panic!("expected ConfigReloaded variant"),
            }
        }

        #[tokio::test]
        async fn config_reloaded_round_trip_through_channel() {
            let channels = Channels::new(16);
            let crate::message::Channels { state_tx, mut state_rx, .. } = channels;

            let config = AppConfig::default();
            let delta = ConfigDelta { theme_changed: true, ..ConfigDelta::default() };
            let warnings = vec![ConfigWarning {
                field: "sidebar.width".to_string(),
                message: "clamped to 100".to_string(),
            }];

            state_tx
                .send(StateUpdate::ConfigReloaded {
                    config: Box::new(config.clone()),
                    delta: delta.clone(),
                    warnings: warnings.clone(),
                })
                .await
                .expect("send should succeed");

            let msg = state_rx.recv().await.expect("should receive message");
            match msg {
                StateUpdate::ConfigReloaded { config: c, delta: d, warnings: w } => {
                    assert_eq!(*c, config);
                    assert_eq!(d, delta);
                    assert!(d.theme_changed);
                    assert_eq!(w.len(), 1);
                    assert_eq!(w[0].field, "sidebar.width");
                }
                other => panic!("expected ConfigReloaded, got: {other:?}"),
            }
        }
    }
}

//! Configuration data model — structs, enums, and default values.

use serde::{Deserialize, Serialize};

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
    /// Update check settings.
    pub updates: UpdatesConfig,
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

/// `[updates]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    /// Whether to check for updates on startup. Default: true.
    pub check_on_startup: bool,
    /// Minimum interval between update checks, in hours. Default: 24.
    /// Prevents excessive API calls when Veil is launched frequently.
    pub check_interval_hours: u32,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check_on_startup: true, check_interval_hours: 24 }
    }
}

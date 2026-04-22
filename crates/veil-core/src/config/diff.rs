//! Config diffing — determine what changed between two config versions.

use super::AppConfig;

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

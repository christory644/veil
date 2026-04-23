//! Config reload logic — delta-driven state updates extracted from the event
//! loop for testability without a window or GPU.

use veil_core::config::{AppConfig, ConfigDelta};
use veil_core::keyboard::KeybindingRegistry;
use veil_core::state::AppState;

/// Apply a config reload to app state and keybindings.
/// Returns `true` if a redraw is needed.
pub fn apply_config_reload(
    config: &AppConfig,
    delta: &ConfigDelta,
    app_state: &mut AppState,
    keybindings: &mut KeybindingRegistry,
) -> bool {
    let mut needs_redraw = false;

    if delta.sidebar_changed {
        app_state.apply_config(config);
        needs_redraw = true;
    }

    if delta.keybindings_changed {
        *keybindings = KeybindingRegistry::with_defaults();
        let warnings =
            veil_core::keyboard::apply_keybindings_config(keybindings, &config.keybindings);
        for w in &warnings {
            tracing::warn!("keybinding config warning: {w}");
        }
        needs_redraw = true;
    }

    if delta.font_changed {
        needs_redraw = true;
    }

    if delta.theme_changed {
        needs_redraw = true;
    }

    needs_redraw
}

#[cfg(test)]
mod tests {
    use super::*;
    use veil_core::config::{AppConfig, ConfigDelta, DefaultTab};
    use veil_core::keyboard::{Key, KeyAction, KeyInput, Modifiers};
    use veil_core::state::SidebarTab;

    // --- Empty delta ---

    #[test]
    fn empty_delta_returns_false() {
        let config = AppConfig::default();
        let delta = ConfigDelta::default();
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        let needs_redraw = apply_config_reload(&config, &delta, &mut state, &mut keybindings);

        assert!(!needs_redraw, "empty delta should not need a redraw");
    }

    // --- sidebar_changed ---

    #[test]
    fn sidebar_changed_applies_config_to_state() {
        let mut config = AppConfig::default();
        config.sidebar.width = 400;
        config.sidebar.visible = false;
        config.sidebar.default_tab = DefaultTab::Conversations;

        let delta = ConfigDelta { sidebar_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        let needs_redraw = apply_config_reload(&config, &delta, &mut state, &mut keybindings);

        assert!(needs_redraw, "sidebar_changed should need a redraw");
        assert_eq!(state.sidebar.width_px, 400, "sidebar width should be updated");
        assert!(!state.sidebar.visible, "sidebar visibility should be updated");
        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Conversations,
            "sidebar tab should be updated"
        );
    }

    #[test]
    fn sidebar_changed_returns_true() {
        let config = AppConfig::default();
        let delta = ConfigDelta { sidebar_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        assert!(apply_config_reload(&config, &delta, &mut state, &mut keybindings));
    }

    // --- keybindings_changed ---

    #[test]
    fn keybindings_changed_rebuilds_registry() {
        let mut config = AppConfig::default();
        config.keybindings.toggle_sidebar = Some("ctrl+b".to_string());

        let delta = ConfigDelta { keybindings_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        let needs_redraw = apply_config_reload(&config, &delta, &mut state, &mut keybindings);

        assert!(needs_redraw, "keybindings_changed should need a redraw");

        // Verify the new binding is active
        let ctrl_b = KeyInput {
            key: Key::Character('b'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        assert_eq!(
            keybindings.lookup(&ctrl_b),
            Some(&KeyAction::ToggleSidebar),
            "ctrl+b should be bound to ToggleSidebar after reload"
        );
    }

    #[test]
    fn keybindings_changed_returns_true() {
        let config = AppConfig::default();
        let delta = ConfigDelta { keybindings_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        assert!(apply_config_reload(&config, &delta, &mut state, &mut keybindings));
    }

    // --- font_changed ---

    #[test]
    fn font_changed_returns_true() {
        let config = AppConfig::default();
        let delta = ConfigDelta { font_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        assert!(
            apply_config_reload(&config, &delta, &mut state, &mut keybindings),
            "font_changed should need a redraw"
        );
    }

    // --- theme_changed ---

    #[test]
    fn theme_changed_returns_true() {
        let config = AppConfig::default();
        let delta = ConfigDelta { theme_changed: true, ..ConfigDelta::default() };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        assert!(
            apply_config_reload(&config, &delta, &mut state, &mut keybindings),
            "theme_changed should need a redraw"
        );
    }

    // --- Multiple changes ---

    #[test]
    fn multiple_changes_all_applied_returns_true() {
        let mut config = AppConfig::default();
        config.sidebar.width = 350;
        config.sidebar.default_tab = DefaultTab::Conversations;
        config.keybindings.toggle_sidebar = Some("ctrl+b".to_string());

        let delta = ConfigDelta {
            sidebar_changed: true,
            keybindings_changed: true,
            font_changed: true,
            theme_changed: true,
            ..ConfigDelta::default()
        };
        let mut state = AppState::new();
        let mut keybindings = KeybindingRegistry::with_defaults();

        let needs_redraw = apply_config_reload(&config, &delta, &mut state, &mut keybindings);

        assert!(needs_redraw, "multiple changes should need a redraw");
        assert_eq!(state.sidebar.width_px, 350, "sidebar width should be applied");
        assert_eq!(state.sidebar.active_tab, SidebarTab::Conversations);

        let ctrl_b = KeyInput {
            key: Key::Character('b'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        assert_eq!(keybindings.lookup(&ctrl_b), Some(&KeyAction::ToggleSidebar));
    }

    // --- Non-matching delta does not modify state ---

    #[test]
    fn non_matching_delta_fields_do_not_modify_state() {
        let config = AppConfig::default();
        // Only scrollback_changed and adapters_changed — should not touch sidebar or keybindings
        let delta = ConfigDelta {
            scrollback_changed: true,
            adapters_changed: true,
            ..ConfigDelta::default()
        };
        let mut state = AppState::new();
        let original_width = state.sidebar.width_px;
        let original_visible = state.sidebar.visible;
        let original_tab = state.sidebar.active_tab;
        let mut keybindings = KeybindingRegistry::with_defaults();
        let binding_count_before = keybindings.all_bindings().len();

        let _needs_redraw = apply_config_reload(&config, &delta, &mut state, &mut keybindings);

        assert_eq!(
            state.sidebar.width_px, original_width,
            "scrollback_changed should not modify sidebar width"
        );
        assert_eq!(
            state.sidebar.visible, original_visible,
            "adapters_changed should not modify sidebar visibility"
        );
        assert_eq!(
            state.sidebar.active_tab, original_tab,
            "non-sidebar delta should not modify active tab"
        );
        assert_eq!(
            keybindings.all_bindings().len(),
            binding_count_before,
            "non-keybinding delta should not modify registry"
        );
    }
}

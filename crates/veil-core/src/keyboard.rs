//! Keyboard dispatch — configurable keybinding registry.
//!
//! Maps key combinations to application-level actions. Platform-agnostic;
//! the binary crate translates winit key events into the domain `KeyInput` type.

/// Modifier keys held during a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Control key.
    pub ctrl: bool,
    /// Shift key.
    pub shift: bool,
    /// Alt/Option key.
    pub alt: bool,
    /// Cmd on macOS, Win/Super on Linux/Windows.
    pub logo: bool,
}

/// A physical or logical key identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A named key (Enter, Tab, Escape, F1-F12, Arrow keys, etc.)
    Named(String),
    /// A character key.
    Character(char),
}

/// A complete key input event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyInput {
    /// The key that was pressed.
    pub key: Key,
    /// Modifiers held during the press.
    pub modifiers: Modifiers,
}

/// Actions that the keyboard dispatch system can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Switch to workspace N (1-9).
    SwitchWorkspace(u8),
    /// Create a new workspace.
    CreateWorkspace,
    /// Close the current workspace.
    CloseWorkspace,
    /// Split the current pane horizontally.
    SplitHorizontal,
    /// Split the current pane vertically.
    SplitVertical,
    /// Close the current pane.
    ClosePane,
    /// Focus the next pane.
    FocusNextPane,
    /// Focus the previous pane.
    FocusPreviousPane,
    /// Zoom the current pane.
    ZoomPane,
    /// Toggle sidebar visibility.
    ToggleSidebar,
    /// Switch to Workspaces tab.
    SwitchToWorkspacesTab,
    /// Switch to Conversations tab.
    SwitchToConversationsTab,
    /// Focus the sidebar.
    FocusSidebar,
    /// Focus the terminal.
    FocusTerminal,
}

/// A binding from a key combination to an action.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// The key input that triggers this binding.
    pub input: KeyInput,
    /// The action to perform.
    pub action: KeyAction,
}

/// Registry of keybindings. Supports lookup by key input.
pub struct KeybindingRegistry {
    bindings: Vec<KeyBinding>,
}

impl KeybindingRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { bindings: Vec::new() }
    }

    /// Create a registry populated with default keybindings.
    pub fn with_defaults() -> Self {
        todo!()
    }

    /// Add or replace a binding.
    pub fn bind(&mut self, _input: KeyInput, _action: KeyAction) {
        todo!()
    }

    /// Remove a binding. Returns the old action if it existed.
    pub fn unbind(&mut self, _input: &KeyInput) -> Option<KeyAction> {
        todo!()
    }

    /// Find the action for a key input.
    pub fn lookup(&self, _input: &KeyInput) -> Option<&KeyAction> {
        todo!()
    }

    /// List all bindings.
    pub fn all_bindings(&self) -> &[KeyBinding] {
        todo!()
    }

    /// Remove all bindings.
    pub fn clear(&mut self) {
        todo!()
    }
}

impl Default for KeybindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logo_key(c: char) -> KeyInput {
        KeyInput {
            key: Key::Character(c),
            modifiers: Modifiers { logo: true, ..Default::default() },
        }
    }

    fn logo_shift_key(c: char) -> KeyInput {
        KeyInput {
            key: Key::Character(c),
            modifiers: Modifiers { logo: true, shift: true, ..Default::default() },
        }
    }

    fn ctrl_shift_key(c: char) -> KeyInput {
        KeyInput {
            key: Key::Character(c),
            modifiers: Modifiers { ctrl: true, shift: true, ..Default::default() },
        }
    }

    fn logo_named(name: &str) -> KeyInput {
        KeyInput {
            key: Key::Named(name.to_string()),
            modifiers: Modifiers { logo: true, ..Default::default() },
        }
    }

    fn logo_shift_named(name: &str) -> KeyInput {
        KeyInput {
            key: Key::Named(name.to_string()),
            modifiers: Modifiers { logo: true, shift: true, ..Default::default() },
        }
    }

    // --- Empty registry ---

    #[test]
    fn empty_registry_lookup_returns_none() {
        let registry = KeybindingRegistry::new();
        let input = logo_key('n');
        assert!(registry.lookup(&input).is_none());
    }

    // --- with_defaults ---

    #[test]
    fn with_defaults_populates_bindings() {
        let registry = KeybindingRegistry::with_defaults();
        assert!(!registry.all_bindings().is_empty());
    }

    // --- bind + lookup round-trip ---

    #[test]
    fn bind_and_lookup_round_trip() {
        let mut registry = KeybindingRegistry::new();
        let input = logo_key('n');
        registry.bind(input.clone(), KeyAction::CreateWorkspace);
        let action = registry.lookup(&input);
        assert_eq!(action, Some(&KeyAction::CreateWorkspace));
    }

    #[test]
    fn bind_replaces_existing_binding() {
        let mut registry = KeybindingRegistry::new();
        let input = logo_key('n');
        registry.bind(input.clone(), KeyAction::CreateWorkspace);
        registry.bind(input.clone(), KeyAction::CloseWorkspace);
        let action = registry.lookup(&input);
        assert_eq!(action, Some(&KeyAction::CloseWorkspace));
    }

    // --- unbind ---

    #[test]
    fn unbind_returns_old_action() {
        let mut registry = KeybindingRegistry::new();
        let input = logo_key('n');
        registry.bind(input.clone(), KeyAction::CreateWorkspace);
        let old = registry.unbind(&input);
        assert_eq!(old, Some(KeyAction::CreateWorkspace));
        assert!(registry.lookup(&input).is_none());
    }

    #[test]
    fn unbind_nonexistent_returns_none() {
        let mut registry = KeybindingRegistry::new();
        let input = logo_key('x');
        assert!(registry.unbind(&input).is_none());
    }

    // --- Modifier sensitivity ---

    #[test]
    fn lookup_with_wrong_modifiers_returns_none() {
        let mut registry = KeybindingRegistry::new();
        let logo_n = logo_key('n');
        registry.bind(logo_n, KeyAction::CreateWorkspace);
        // Ctrl+N should NOT match Logo+N
        let ctrl_n = KeyInput {
            key: Key::Character('n'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        assert!(registry.lookup(&ctrl_n).is_none());
    }

    // --- Case sensitivity ---

    #[test]
    fn lookup_is_case_sensitive_for_characters() {
        let mut registry = KeybindingRegistry::new();
        let lower = KeyInput { key: Key::Character('n'), modifiers: Modifiers::default() };
        registry.bind(lower.clone(), KeyAction::CreateWorkspace);
        let upper = KeyInput { key: Key::Character('N'), modifiers: Modifiers::default() };
        assert!(registry.lookup(&upper).is_none());
    }

    // --- all_bindings ---

    #[test]
    fn all_bindings_returns_registered() {
        let mut registry = KeybindingRegistry::new();
        registry.bind(logo_key('n'), KeyAction::CreateWorkspace);
        registry.bind(logo_key('w'), KeyAction::ClosePane);
        assert_eq!(registry.all_bindings().len(), 2);
    }

    // --- clear ---

    #[test]
    fn clear_empties_registry() {
        let mut registry = KeybindingRegistry::new();
        registry.bind(logo_key('n'), KeyAction::CreateWorkspace);
        registry.clear();
        assert!(registry.all_bindings().is_empty());
        assert!(registry.lookup(&logo_key('n')).is_none());
    }

    // --- Default bindings coverage ---

    #[test]
    fn defaults_include_workspace_shortcuts_1_through_9() {
        let registry = KeybindingRegistry::with_defaults();
        for i in 1..=9u8 {
            let input = KeyInput {
                key: Key::Character(char::from(b'0' + i)),
                modifiers: Modifiers { logo: true, ..Default::default() },
            };
            let action =
                registry.lookup(&input).unwrap_or_else(|| panic!("expected binding for Logo+{i}"));
            assert_eq!(*action, KeyAction::SwitchWorkspace(i));
        }
    }

    #[test]
    fn defaults_include_create_workspace() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_key('n')), Some(&KeyAction::CreateWorkspace));
    }

    #[test]
    fn defaults_include_split_horizontal() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_key('d')), Some(&KeyAction::SplitHorizontal));
    }

    #[test]
    fn defaults_include_split_vertical() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_shift_key('d')), Some(&KeyAction::SplitVertical));
    }

    #[test]
    fn defaults_include_close_pane() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_key('w')), Some(&KeyAction::ClosePane));
    }

    #[test]
    fn defaults_include_focus_previous_pane() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_named("[")), Some(&KeyAction::FocusPreviousPane));
    }

    #[test]
    fn defaults_include_focus_next_pane() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_named("]")), Some(&KeyAction::FocusNextPane));
    }

    #[test]
    fn defaults_include_zoom_pane() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_shift_named("Enter")), Some(&KeyAction::ZoomPane));
    }

    #[test]
    fn defaults_include_toggle_sidebar() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_key('b')), Some(&KeyAction::ToggleSidebar));
    }

    #[test]
    fn defaults_include_switch_to_workspaces_tab() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&ctrl_shift_key('w')), Some(&KeyAction::SwitchToWorkspacesTab));
    }

    #[test]
    fn defaults_include_switch_to_conversations_tab() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(
            registry.lookup(&ctrl_shift_key('c')),
            Some(&KeyAction::SwitchToConversationsTab)
        );
    }

    // --- Named and character key lookups ---

    #[test]
    fn named_keys_work_in_lookups() {
        let mut registry = KeybindingRegistry::new();
        let input =
            KeyInput { key: Key::Named("Escape".to_string()), modifiers: Modifiers::default() };
        registry.bind(input.clone(), KeyAction::FocusTerminal);
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusTerminal));
    }

    #[test]
    fn character_keys_work_in_lookups() {
        let mut registry = KeybindingRegistry::new();
        let input = KeyInput {
            key: Key::Character('z'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        registry.bind(input.clone(), KeyAction::FocusSidebar);
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusSidebar));
    }
}

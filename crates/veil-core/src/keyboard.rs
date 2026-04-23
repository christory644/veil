//! Keyboard dispatch — configurable keybinding registry.
//!
//! Maps key combinations to application-level actions. Platform-agnostic;
//! the binary crate translates winit key events into the domain `KeyInput` type.

/// Modifier keys held during a key press.
#[allow(clippy::struct_excessive_bools)]
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
    /// Focus the pane to the left.
    FocusPaneLeft,
    /// Focus the pane to the right.
    FocusPaneRight,
    /// Focus the pane above.
    FocusPaneUp,
    /// Focus the pane below.
    FocusPaneDown,
    /// Rename the active workspace.
    RenameWorkspace,
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
    #[allow(clippy::too_many_lines)]
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Logo+1 through Logo+9 -> SwitchWorkspace(1..9)
        for i in 1..=9u8 {
            registry.bind(
                KeyInput {
                    key: Key::Character(char::from(b'0' + i)),
                    modifiers: Modifiers { logo: true, ..Default::default() },
                },
                KeyAction::SwitchWorkspace(i),
            );
        }

        // Logo+N -> CreateWorkspace
        registry.bind(
            KeyInput {
                key: Key::Character('n'),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::CreateWorkspace,
        );

        // Logo+D -> SplitHorizontal
        registry.bind(
            KeyInput {
                key: Key::Character('d'),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::SplitHorizontal,
        );

        // Logo+Shift+D -> SplitVertical
        registry.bind(
            KeyInput {
                key: Key::Character('d'),
                modifiers: Modifiers { logo: true, shift: true, ..Default::default() },
            },
            KeyAction::SplitVertical,
        );

        // Logo+W -> ClosePane
        registry.bind(
            KeyInput {
                key: Key::Character('w'),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::ClosePane,
        );

        // Logo+[ -> FocusPreviousPane
        registry.bind(
            KeyInput {
                key: Key::Character('['),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::FocusPreviousPane,
        );

        // Logo+] -> FocusNextPane
        registry.bind(
            KeyInput {
                key: Key::Character(']'),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::FocusNextPane,
        );

        // Logo+Shift+Enter -> ZoomPane
        registry.bind(
            KeyInput {
                key: Key::Named("Enter".to_string()),
                modifiers: Modifiers { logo: true, shift: true, ..Default::default() },
            },
            KeyAction::ZoomPane,
        );

        // Logo+B -> ToggleSidebar
        registry.bind(
            KeyInput {
                key: Key::Character('b'),
                modifiers: Modifiers { logo: true, ..Default::default() },
            },
            KeyAction::ToggleSidebar,
        );

        // Ctrl+Shift+W -> SwitchToWorkspacesTab
        registry.bind(
            KeyInput {
                key: Key::Character('w'),
                modifiers: Modifiers { ctrl: true, shift: true, ..Default::default() },
            },
            KeyAction::SwitchToWorkspacesTab,
        );

        // Ctrl+Shift+C -> SwitchToConversationsTab
        registry.bind(
            KeyInput {
                key: Key::Character('c'),
                modifiers: Modifiers { ctrl: true, shift: true, ..Default::default() },
            },
            KeyAction::SwitchToConversationsTab,
        );

        // Ctrl+H -> FocusPaneLeft
        registry.bind(
            KeyInput {
                key: Key::Character('h'),
                modifiers: Modifiers { ctrl: true, ..Default::default() },
            },
            KeyAction::FocusPaneLeft,
        );

        // Ctrl+L -> FocusPaneRight
        registry.bind(
            KeyInput {
                key: Key::Character('l'),
                modifiers: Modifiers { ctrl: true, ..Default::default() },
            },
            KeyAction::FocusPaneRight,
        );

        // Ctrl+K -> FocusPaneUp
        registry.bind(
            KeyInput {
                key: Key::Character('k'),
                modifiers: Modifiers { ctrl: true, ..Default::default() },
            },
            KeyAction::FocusPaneUp,
        );

        // Ctrl+J -> FocusPaneDown
        registry.bind(
            KeyInput {
                key: Key::Character('j'),
                modifiers: Modifiers { ctrl: true, ..Default::default() },
            },
            KeyAction::FocusPaneDown,
        );

        registry
    }

    /// Add or replace a binding.
    pub fn bind(&mut self, input: KeyInput, action: KeyAction) {
        if let Some(existing) = self.bindings.iter_mut().find(|b| b.input == input) {
            existing.action = action;
        } else {
            self.bindings.push(KeyBinding { input, action });
        }
    }

    /// Remove a binding. Returns the old action if it existed.
    pub fn unbind(&mut self, input: &KeyInput) -> Option<KeyAction> {
        let pos = self.bindings.iter().position(|b| b.input == *input)?;
        Some(self.bindings.remove(pos).action)
    }

    /// Find the action for a key input.
    pub fn lookup(&self, input: &KeyInput) -> Option<&KeyAction> {
        self.bindings.iter().find(|b| b.input == *input).map(|b| &b.action)
    }

    /// List all bindings.
    pub fn all_bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    /// Remove all bindings.
    pub fn clear(&mut self) {
        self.bindings.clear();
    }
}

impl Default for KeybindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a keybinding string like "ctrl+shift+w" or "cmd+b" into a `KeyInput`.
///
/// Format: `modifier+modifier+key`
/// Modifiers: `ctrl`, `shift`, `alt`, `cmd`/`logo`/`super`
/// Keys: single characters (a-z, 0-9, symbols) or named keys (enter, tab, escape, f1-f12, etc.)
///
/// Returns `None` if the string is empty or unparseable.
pub fn parse_keybinding(s: &str) -> Option<KeyInput> {
    if s.is_empty() {
        return None;
    }

    // Reject leading or trailing '+' (which produce empty parts after split).
    if s.starts_with('+') || s.ends_with('+') {
        return None;
    }

    let parts: Vec<&str> = s.split('+').collect();

    // Every part must be non-empty (catches "a++b" style input).
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }

    let mut modifiers = Modifiers::default();
    let mut key_part: Option<Key> = None;

    for part in &parts {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            // Modifiers
            "ctrl" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            "cmd" | "logo" | "super" => modifiers.logo = true,
            // Named keys
            other => {
                // If we already found a key, this part is unexpected —
                // treat the last non-modifier part as the key (overwrite).
                key_part = Some(parse_key_name(other)?);
            }
        }
    }

    let key = key_part?;
    Some(KeyInput { key, modifiers })
}

/// Map a lowercase key name to a `Key`, returning `None` for unknown names.
fn parse_key_name(name: &str) -> Option<Key> {
    // Named keys — return canonical capitalization.
    let named = match name {
        "enter" => "Enter",
        "tab" => "Tab",
        "escape" => "Escape",
        "space" => "Space",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        _ => {
            // Single character key — normalize to lowercase.
            let chars: Vec<char> = name.chars().collect();
            if chars.len() == 1 {
                return Some(Key::Character(chars[0].to_ascii_lowercase()));
            }
            // Multi-character unknown name — not parseable.
            return None;
        }
    };
    Some(Key::Named(named.to_string()))
}

/// Apply keybindings from config to the registry.
///
/// For each non-None field in `KeybindingsConfig`, parses the keybinding
/// string and rebinds the corresponding action. Unknown/unparseable strings
/// are logged as warnings and skipped.
///
/// Returns a list of warnings for keybinding strings that could not be parsed.
pub fn apply_keybindings_config(
    registry: &mut KeybindingRegistry,
    config: &crate::config::KeybindingsConfig,
) -> Vec<String> {
    let mut warnings = Vec::new();

    let mappings: &[(&Option<String>, KeyAction)] = &[
        (&config.toggle_sidebar, KeyAction::ToggleSidebar),
        (&config.workspace_tab, KeyAction::SwitchToWorkspacesTab),
        (&config.conversations_tab, KeyAction::SwitchToConversationsTab),
        (&config.new_workspace, KeyAction::CreateWorkspace),
        (&config.close_workspace, KeyAction::CloseWorkspace),
        (&config.split_horizontal, KeyAction::SplitHorizontal),
        (&config.split_vertical, KeyAction::SplitVertical),
        (&config.close_pane, KeyAction::ClosePane),
        (&config.focus_next_pane, KeyAction::FocusNextPane),
        (&config.focus_previous_pane, KeyAction::FocusPreviousPane),
        (&config.zoom_pane, KeyAction::ZoomPane),
        (&config.focus_pane_left, KeyAction::FocusPaneLeft),
        (&config.focus_pane_right, KeyAction::FocusPaneRight),
        (&config.focus_pane_up, KeyAction::FocusPaneUp),
        (&config.focus_pane_down, KeyAction::FocusPaneDown),
    ];

    for (field, action) in mappings {
        if let Some(binding_str) = field {
            match parse_keybinding(binding_str) {
                Some(key_input) => {
                    registry.bind(key_input, action.clone());
                }
                None => {
                    warnings.push(format!(
                        "could not parse keybinding \"{binding_str}\" for {action:?}"
                    ));
                }
            }
        }
    }

    warnings
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

    fn ctrl_key(c: char) -> KeyInput {
        KeyInput {
            key: Key::Character(c),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
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
        assert_eq!(registry.lookup(&logo_key('[')), Some(&KeyAction::FocusPreviousPane));
    }

    #[test]
    fn defaults_include_focus_next_pane() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(registry.lookup(&logo_key(']')), Some(&KeyAction::FocusNextPane));
    }

    // VEI-75 Unit 5: old named key form no longer matches
    #[test]
    fn old_named_bracket_bindings_no_longer_match() {
        let registry = KeybindingRegistry::with_defaults();
        assert_eq!(
            registry.lookup(&logo_named("[")),
            None,
            "Key::Named(\"[\") should no longer match after fix"
        );
        assert_eq!(
            registry.lookup(&logo_named("]")),
            None,
            "Key::Named(\"]\") should no longer match after fix"
        );
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

    // --- VEI-11: Directional focus default bindings ---

    #[test]
    fn defaults_include_focus_pane_left() {
        let registry = KeybindingRegistry::with_defaults();
        let input = ctrl_key('h');
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusPaneLeft));
    }

    #[test]
    fn defaults_include_focus_pane_right() {
        let registry = KeybindingRegistry::with_defaults();
        let input = ctrl_key('l');
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusPaneRight));
    }

    #[test]
    fn defaults_include_focus_pane_up() {
        let registry = KeybindingRegistry::with_defaults();
        let input = ctrl_key('k');
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusPaneUp));
    }

    #[test]
    fn defaults_include_focus_pane_down() {
        let registry = KeybindingRegistry::with_defaults();
        let input = ctrl_key('j');
        assert_eq!(registry.lookup(&input), Some(&KeyAction::FocusPaneDown));
    }

    // --- VEI-11: RenameWorkspace variant exists ---

    #[test]
    fn rename_workspace_variant_can_be_bound() {
        let mut registry = KeybindingRegistry::new();
        let input = logo_key('r');
        registry.bind(input.clone(), KeyAction::RenameWorkspace);
        assert_eq!(registry.lookup(&input), Some(&KeyAction::RenameWorkspace));
    }

    #[test]
    fn rename_workspace_has_no_default_binding() {
        let registry = KeybindingRegistry::with_defaults();
        // RenameWorkspace should not appear in any default binding
        let has_rename =
            registry.all_bindings().iter().any(|b| b.action == KeyAction::RenameWorkspace);
        assert!(!has_rename, "RenameWorkspace should have no default binding");
    }
}

// ============================================================
// VEI-79: parse_keybinding — string-to-KeyInput parsing
// ============================================================

#[cfg(test)]
mod parse_keybinding_tests {
    use super::*;

    // --- Basic modifier+character combos ---

    #[test]
    fn parse_ctrl_shift_w() {
        let result = parse_keybinding("ctrl+shift+w");
        assert!(result.is_some(), "ctrl+shift+w should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Character('w'));
        assert!(ki.modifiers.ctrl, "ctrl should be set");
        assert!(ki.modifiers.shift, "shift should be set");
        assert!(!ki.modifiers.alt, "alt should not be set");
        assert!(!ki.modifiers.logo, "logo should not be set");
    }

    #[test]
    fn parse_cmd_b() {
        let result = parse_keybinding("cmd+b");
        assert!(result.is_some(), "cmd+b should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Character('b'));
        assert!(ki.modifiers.logo, "cmd should map to logo modifier");
        assert!(!ki.modifiers.ctrl);
        assert!(!ki.modifiers.shift);
        assert!(!ki.modifiers.alt);
    }

    #[test]
    fn parse_logo_b_same_as_cmd_b() {
        let cmd = parse_keybinding("cmd+b").expect("cmd+b should parse");
        let logo = parse_keybinding("logo+b").expect("logo+b should parse");
        assert_eq!(cmd, logo, "cmd+b and logo+b should produce the same KeyInput");
    }

    #[test]
    fn parse_super_b_same_as_cmd_b() {
        let cmd = parse_keybinding("cmd+b").expect("cmd+b should parse");
        let sup = parse_keybinding("super+b").expect("super+b should parse");
        assert_eq!(cmd, sup, "cmd+b and super+b should produce the same KeyInput");
    }

    #[test]
    fn parse_alt_n() {
        let result = parse_keybinding("alt+n");
        assert!(result.is_some(), "alt+n should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Character('n'));
        assert!(ki.modifiers.alt, "alt should be set");
        assert!(!ki.modifiers.ctrl);
        assert!(!ki.modifiers.shift);
        assert!(!ki.modifiers.logo);
    }

    // --- Named keys ---

    #[test]
    fn parse_ctrl_shift_enter() {
        let result = parse_keybinding("ctrl+shift+enter");
        assert!(result.is_some(), "ctrl+shift+enter should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Enter".to_string()));
        assert!(ki.modifiers.ctrl);
        assert!(ki.modifiers.shift);
    }

    #[test]
    fn parse_f1_no_modifiers() {
        let result = parse_keybinding("f1");
        assert!(result.is_some(), "f1 should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("F1".to_string()));
        assert_eq!(ki.modifiers, Modifiers::default());
    }

    #[test]
    fn parse_ctrl_f5() {
        let result = parse_keybinding("ctrl+f5");
        assert!(result.is_some(), "ctrl+f5 should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("F5".to_string()));
        assert!(ki.modifiers.ctrl);
    }

    #[test]
    fn parse_escape() {
        let result = parse_keybinding("escape");
        assert!(result.is_some(), "escape should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Escape".to_string()));
        assert_eq!(ki.modifiers, Modifiers::default());
    }

    #[test]
    fn parse_tab() {
        let result = parse_keybinding("tab");
        assert!(result.is_some(), "tab should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Tab".to_string()));
        assert_eq!(ki.modifiers, Modifiers::default());
    }

    #[test]
    fn parse_space() {
        let result = parse_keybinding("space");
        assert!(result.is_some(), "space should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Space".to_string()));
    }

    #[test]
    fn parse_backspace() {
        let result = parse_keybinding("backspace");
        assert!(result.is_some(), "backspace should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Backspace".to_string()));
    }

    #[test]
    fn parse_delete() {
        let result = parse_keybinding("delete");
        assert!(result.is_some(), "delete should parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Named("Delete".to_string()));
    }

    #[test]
    fn parse_arrow_keys() {
        for (name, expected) in
            [("up", "Up"), ("down", "Down"), ("left", "Left"), ("right", "Right")]
        {
            let result = parse_keybinding(name);
            assert!(result.is_some(), "{name} should parse");
            assert_eq!(result.unwrap().key, Key::Named(expected.to_string()));
        }
    }

    #[test]
    fn parse_home_end() {
        let home = parse_keybinding("home").expect("home should parse");
        assert_eq!(home.key, Key::Named("Home".to_string()));
        let end = parse_keybinding("end").expect("end should parse");
        assert_eq!(end.key, Key::Named("End".to_string()));
    }

    #[test]
    fn parse_pageup_pagedown() {
        let pgup = parse_keybinding("pageup").expect("pageup should parse");
        assert_eq!(pgup.key, Key::Named("PageUp".to_string()));
        let pgdn = parse_keybinding("pagedown").expect("pagedown should parse");
        assert_eq!(pgdn.key, Key::Named("PageDown".to_string()));
    }

    // --- Case insensitivity ---

    #[test]
    fn parse_uppercase_char_normalized_to_lowercase() {
        let result = parse_keybinding("ctrl+shift+B");
        assert!(result.is_some(), "ctrl+shift+B should parse");
        let ki = result.unwrap();
        assert_eq!(
            ki.key,
            Key::Character('b'),
            "uppercase character should be normalized to lowercase"
        );
        assert!(ki.modifiers.ctrl);
        assert!(ki.modifiers.shift);
    }

    // --- Edge cases and invalid input ---

    #[test]
    fn parse_empty_string_returns_none() {
        assert!(parse_keybinding("").is_none(), "empty string should return None");
    }

    #[test]
    fn parse_trailing_plus_returns_none() {
        assert!(parse_keybinding("ctrl+").is_none(), "trailing plus should return None");
    }

    #[test]
    fn parse_leading_plus_returns_none() {
        assert!(parse_keybinding("+b").is_none(), "leading plus should return None");
    }

    #[test]
    fn parse_duplicate_modifier_still_valid() {
        let result = parse_keybinding("ctrl+ctrl+b");
        assert!(result.is_some(), "duplicate modifier should still parse");
        let ki = result.unwrap();
        assert_eq!(ki.key, Key::Character('b'));
        assert!(ki.modifiers.ctrl);
    }

    #[test]
    fn parse_invalid_single_word_returns_none() {
        // "invalid" is not a known key name and has no '+' separator
        assert!(parse_keybinding("invalid").is_none(), "unknown single word should return None");
    }
}

// ============================================================
// VEI-79: apply_keybindings_config — config-to-registry binding
// ============================================================

#[cfg(test)]
mod apply_keybindings_config_tests {
    use super::*;
    use crate::config::KeybindingsConfig;

    #[test]
    fn default_config_produces_no_warnings_and_no_changes() {
        let mut registry = KeybindingRegistry::with_defaults();
        let binding_count_before = registry.all_bindings().len();
        let config = KeybindingsConfig::default(); // all fields None

        let warnings = apply_keybindings_config(&mut registry, &config);

        assert!(warnings.is_empty(), "default config should produce no warnings");
        assert_eq!(
            registry.all_bindings().len(),
            binding_count_before,
            "default config should not change binding count"
        );
    }

    #[test]
    fn setting_toggle_sidebar_overrides_binding() {
        let mut registry = KeybindingRegistry::with_defaults();
        let config = KeybindingsConfig {
            toggle_sidebar: Some("ctrl+b".to_string()),
            ..KeybindingsConfig::default()
        };

        let warnings = apply_keybindings_config(&mut registry, &config);
        assert!(warnings.is_empty(), "valid keybinding should produce no warnings");

        // The new binding should work
        let new_input = KeyInput {
            key: Key::Character('b'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        assert_eq!(
            registry.lookup(&new_input),
            Some(&KeyAction::ToggleSidebar),
            "ctrl+b should now be bound to ToggleSidebar"
        );
    }

    #[test]
    fn unparseable_string_produces_warning_and_does_not_crash() {
        let mut registry = KeybindingRegistry::with_defaults();
        let config = KeybindingsConfig {
            toggle_sidebar: Some("+++garbage+++".to_string()),
            ..KeybindingsConfig::default()
        };

        let warnings = apply_keybindings_config(&mut registry, &config);

        assert!(!warnings.is_empty(), "unparseable keybinding should produce a warning");
    }

    #[test]
    fn setting_multiple_fields_overrides_each_binding() {
        let mut registry = KeybindingRegistry::with_defaults();
        let config = KeybindingsConfig {
            toggle_sidebar: Some("ctrl+b".to_string()),
            new_workspace: Some("ctrl+n".to_string()),
            split_horizontal: Some("ctrl+d".to_string()),
            ..KeybindingsConfig::default()
        };

        let warnings = apply_keybindings_config(&mut registry, &config);
        assert!(warnings.is_empty(), "all valid keybindings should produce no warnings");

        let ctrl_b = KeyInput {
            key: Key::Character('b'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        let ctrl_n = KeyInput {
            key: Key::Character('n'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };
        let ctrl_d = KeyInput {
            key: Key::Character('d'),
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        };

        assert_eq!(registry.lookup(&ctrl_b), Some(&KeyAction::ToggleSidebar));
        assert_eq!(registry.lookup(&ctrl_n), Some(&KeyAction::CreateWorkspace));
        assert_eq!(registry.lookup(&ctrl_d), Some(&KeyAction::SplitHorizontal));
    }

    #[test]
    fn unset_fields_leave_defaults_untouched() {
        let defaults = KeybindingRegistry::with_defaults();
        let mut registry = KeybindingRegistry::with_defaults();

        // Only set toggle_sidebar, leave everything else as None
        let config = KeybindingsConfig {
            toggle_sidebar: Some("ctrl+b".to_string()),
            ..KeybindingsConfig::default()
        };

        let _ = apply_keybindings_config(&mut registry, &config);

        // The default Logo+N -> CreateWorkspace binding should still work
        let logo_n = KeyInput {
            key: Key::Character('n'),
            modifiers: Modifiers { logo: true, ..Default::default() },
        };
        assert_eq!(
            registry.lookup(&logo_n),
            defaults.lookup(&logo_n),
            "unset config fields should leave default bindings untouched"
        );

        // Ctrl+Shift+W -> SwitchToWorkspacesTab default should still work
        let ctrl_shift_w = KeyInput {
            key: Key::Character('w'),
            modifiers: Modifiers { ctrl: true, shift: true, ..Default::default() },
        };
        assert_eq!(
            registry.lookup(&ctrl_shift_w),
            defaults.lookup(&ctrl_shift_w),
            "unset config fields should leave default bindings untouched"
        );
    }
}

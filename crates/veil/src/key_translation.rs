//! winit key event translation -- converts winit key types into veil-core
//! domain types and PTY byte sequences.
//!
//! The public API takes `winit::event::KeyEvent` references, but since
//! `KeyEvent` cannot be constructed outside the winit crate (due to
//! `pub(crate)` fields), the actual logic lives in helper functions that
//! operate on `winit::keyboard::Key` and `ElementState` directly. These
//! helpers are tested thoroughly; the thin `KeyEvent` wrappers are verified
//! via integration (event loop wiring in `main.rs`).

use veil_core::keyboard;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

/// Convert a winit `KeyEvent` into a domain `KeyInput`.
///
/// Returns `None` for key releases, modifier-only presses, or keys we cannot
/// meaningfully translate.
pub fn translate_key_event(
    event: &KeyEvent,
    modifiers: keyboard::Modifiers,
) -> Option<keyboard::KeyInput> {
    if event.state == ElementState::Released {
        return None;
    }
    translate_logical_key(&event.logical_key, modifiers)
}

/// Convert a winit logical key into a domain `KeyInput`.
///
/// Returns `None` for modifier-only keys or keys we cannot translate.
pub fn translate_logical_key(
    key: &Key,
    modifiers: keyboard::Modifiers,
) -> Option<keyboard::KeyInput> {
    match key {
        Key::Character(text) => {
            let c = text.chars().next()?;
            // Lowercase when logo or ctrl modifier is held (for keybinding matching).
            let c = if modifiers.logo || modifiers.ctrl {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c
            };
            Some(keyboard::KeyInput { key: keyboard::Key::Character(c), modifiers })
        }
        Key::Named(named) => {
            let name = match named {
                NamedKey::Enter => "Enter",
                NamedKey::Tab => "Tab",
                NamedKey::Escape => "Escape",
                NamedKey::Backspace => "Backspace",
                NamedKey::ArrowUp => "ArrowUp",
                NamedKey::ArrowDown => "ArrowDown",
                NamedKey::ArrowLeft => "ArrowLeft",
                NamedKey::ArrowRight => "ArrowRight",
                NamedKey::Home => "Home",
                NamedKey::End => "End",
                NamedKey::Delete => "Delete",
                NamedKey::PageUp => "PageUp",
                NamedKey::PageDown => "PageDown",
                NamedKey::Space => "Space",
                NamedKey::Insert => "Insert",
                NamedKey::F1 => "F1",
                NamedKey::F2 => "F2",
                NamedKey::F3 => "F3",
                NamedKey::F4 => "F4",
                NamedKey::F5 => "F5",
                NamedKey::F6 => "F6",
                NamedKey::F7 => "F7",
                NamedKey::F8 => "F8",
                NamedKey::F9 => "F9",
                NamedKey::F10 => "F10",
                NamedKey::F11 => "F11",
                NamedKey::F12 => "F12",
                // Modifier-only keys -> None
                NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::Super
                | NamedKey::Meta => {
                    return None;
                }
                // Unknown/unsupported named keys -> None
                _ => return None,
            };
            Some(keyboard::KeyInput { key: keyboard::Key::Named(name.to_string()), modifiers })
        }
        // Dead keys, unidentified, etc. -> None
        _ => None,
    }
}

/// Convert a winit `Modifiers` struct into our domain `Modifiers`.
pub fn translate_modifiers(state: winit::event::Modifiers) -> keyboard::Modifiers {
    let s = state.state();
    keyboard::Modifiers {
        logo: s.super_key(),
        ctrl: s.control_key(),
        shift: s.shift_key(),
        alt: s.alt_key(),
    }
}

/// Encode a key event as bytes to send to the PTY.
///
/// Returns `None` if the key has no byte representation (e.g., modifier-only keys).
pub fn key_to_pty_bytes(event: &KeyEvent, modifiers: keyboard::Modifiers) -> Option<Vec<u8>> {
    if event.state == ElementState::Released {
        return None;
    }
    key_to_pty_bytes_from_key(&event.logical_key, modifiers)
}

/// Encode a logical key as bytes to send to the PTY.
///
/// Returns `None` if the key has no byte representation.
pub fn key_to_pty_bytes_from_key(key: &Key, modifiers: keyboard::Modifiers) -> Option<Vec<u8>> {
    match key {
        Key::Character(text) => {
            let first_char = text.chars().next()?;
            // Ctrl+letter (a-z) produces control codes. Logo/Cmd does NOT.
            if modifiers.ctrl && first_char.is_ascii_lowercase() {
                let code = (first_char as u8) - 0x60;
                Some(vec![code])
            } else {
                // UTF-8 encode the text.
                Some(text.as_bytes().to_vec())
            }
        }
        Key::Named(named) => match named {
            NamedKey::Enter => Some(vec![0x0D]),
            NamedKey::Tab => Some(vec![0x09]),
            NamedKey::Escape => Some(vec![0x1B]),
            NamedKey::Backspace => Some(vec![0x7F]),
            NamedKey::Space => Some(vec![0x20]),
            NamedKey::ArrowUp => Some(vec![0x1B, b'[', b'A']),
            NamedKey::ArrowDown => Some(vec![0x1B, b'[', b'B']),
            NamedKey::ArrowRight => Some(vec![0x1B, b'[', b'C']),
            NamedKey::ArrowLeft => Some(vec![0x1B, b'[', b'D']),
            NamedKey::Home => Some(vec![0x1B, b'[', b'H']),
            NamedKey::End => Some(vec![0x1B, b'[', b'F']),
            NamedKey::Delete => Some(vec![0x1B, b'[', b'3', b'~']),
            NamedKey::PageUp => Some(vec![0x1B, b'[', b'5', b'~']),
            NamedKey::PageDown => Some(vec![0x1B, b'[', b'6', b'~']),
            NamedKey::Insert => Some(vec![0x1B, b'[', b'2', b'~']),
            NamedKey::F1 => Some(vec![0x1B, b'O', b'P']),
            NamedKey::F2 => Some(vec![0x1B, b'O', b'Q']),
            NamedKey::F3 => Some(vec![0x1B, b'O', b'R']),
            NamedKey::F4 => Some(vec![0x1B, b'O', b'S']),
            NamedKey::F5 => Some(b"\x1b[15~".to_vec()),
            NamedKey::F6 => Some(b"\x1b[17~".to_vec()),
            NamedKey::F7 => Some(b"\x1b[18~".to_vec()),
            NamedKey::F8 => Some(b"\x1b[19~".to_vec()),
            NamedKey::F9 => Some(b"\x1b[20~".to_vec()),
            NamedKey::F10 => Some(b"\x1b[21~".to_vec()),
            NamedKey::F11 => Some(b"\x1b[23~".to_vec()),
            NamedKey::F12 => Some(b"\x1b[24~".to_vec()),
            // Modifier-only or unknown -> None
            _ => None,
        },
        // Dead keys, unidentified, etc.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veil_core::keyboard::{self, Modifiers};
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    // ================================================================
    // Helpers
    // ================================================================

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl_mods() -> Modifiers {
        Modifiers { ctrl: true, ..Default::default() }
    }

    fn logo_mods() -> Modifiers {
        Modifiers { logo: true, ..Default::default() }
    }

    fn shift_mods() -> Modifiers {
        Modifiers { shift: true, ..Default::default() }
    }

    fn char_key(c: &str) -> Key {
        Key::Character(c.into())
    }

    fn named_key(n: NamedKey) -> Key {
        Key::Named(n)
    }

    fn winit_mods(state: ModifiersState) -> winit::event::Modifiers {
        winit::event::Modifiers::from(state)
    }

    // ================================================================
    // Unit 1, Test 1: Character key translation
    // ================================================================

    #[test]
    fn translate_character_key_a() {
        let result = translate_logical_key(&char_key("a"), no_mods());
        assert!(result.is_some(), "character key 'a' should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Character('a'));
        assert_eq!(input.modifiers, no_mods());
    }

    // ================================================================
    // Unit 1, Test 2: Named key translation (Enter)
    // ================================================================

    #[test]
    fn translate_named_key_enter() {
        let result = translate_logical_key(&named_key(NamedKey::Enter), no_mods());
        assert!(result.is_some(), "Enter should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Named("Enter".to_string()));
    }

    // ================================================================
    // Unit 1, Test 4: Modifier-only key ignored
    // ================================================================

    #[test]
    fn translate_modifier_only_shift_returns_none() {
        let result = translate_logical_key(&named_key(NamedKey::Shift), no_mods());
        assert!(result.is_none(), "modifier-only key Shift should return None");
    }

    #[test]
    fn translate_modifier_only_control_returns_none() {
        let result = translate_logical_key(&named_key(NamedKey::Control), no_mods());
        assert!(result.is_none(), "modifier-only key Control should return None");
    }

    #[test]
    fn translate_modifier_only_alt_returns_none() {
        let result = translate_logical_key(&named_key(NamedKey::Alt), no_mods());
        assert!(result.is_none(), "modifier-only key Alt should return None");
    }

    #[test]
    fn translate_modifier_only_super_returns_none() {
        let result = translate_logical_key(&named_key(NamedKey::Super), no_mods());
        assert!(result.is_none(), "modifier-only key Super should return None");
    }

    // ================================================================
    // Unit 1, Test 5: Modifiers correctly mapped
    // ================================================================

    #[test]
    fn translate_modifiers_logo_flag() {
        let result = translate_modifiers(winit_mods(ModifiersState::SUPER));
        assert!(result.logo, "SUPER flag should map to logo=true");
        assert!(!result.ctrl);
        assert!(!result.shift);
        assert!(!result.alt);
    }

    #[test]
    fn translate_modifiers_ctrl_flag() {
        let result = translate_modifiers(winit_mods(ModifiersState::CONTROL));
        assert!(result.ctrl, "CONTROL flag should map to ctrl=true");
        assert!(!result.logo);
    }

    #[test]
    fn translate_modifiers_shift_flag() {
        let result = translate_modifiers(winit_mods(ModifiersState::SHIFT));
        assert!(result.shift, "SHIFT flag should map to shift=true");
    }

    #[test]
    fn translate_modifiers_alt_flag() {
        let result = translate_modifiers(winit_mods(ModifiersState::ALT));
        assert!(result.alt, "ALT flag should map to alt=true");
    }

    #[test]
    fn translate_modifiers_multiple_flags() {
        let result =
            translate_modifiers(winit_mods(ModifiersState::CONTROL | ModifiersState::SHIFT));
        assert!(result.ctrl);
        assert!(result.shift);
        assert!(!result.logo);
        assert!(!result.alt);
    }

    #[test]
    fn translate_modifiers_none_set() {
        let result = translate_modifiers(winit_mods(ModifiersState::empty()));
        assert!(!result.ctrl);
        assert!(!result.shift);
        assert!(!result.alt);
        assert!(!result.logo);
    }

    // ================================================================
    // Character key with modifiers preserves them
    // ================================================================

    #[test]
    fn translate_character_key_with_logo_modifier() {
        let result = translate_logical_key(&char_key("d"), logo_mods());
        assert!(result.is_some(), "character key 'd' with logo should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Character('d'));
        assert!(input.modifiers.logo);
    }

    // ================================================================
    // Unit 2, Test 1: Character PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_character_a() {
        let result = key_to_pty_bytes_from_key(&char_key("a"), no_mods());
        assert_eq!(result, Some(vec![0x61]));
    }

    // ================================================================
    // Unit 2, Test 2: Enter PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_enter() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Enter), no_mods());
        assert_eq!(result, Some(vec![0x0D]));
    }

    // ================================================================
    // Unit 2, Test 3: Backspace PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_backspace() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Backspace), no_mods());
        assert_eq!(result, Some(vec![0x7F]));
    }

    // ================================================================
    // Unit 2, Test 4: Arrow key PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_arrow_up() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::ArrowUp), no_mods());
        assert_eq!(result, Some(vec![0x1B, 0x5B, 0x41]));
    }

    #[test]
    fn pty_bytes_arrow_down() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::ArrowDown), no_mods());
        assert_eq!(result, Some(vec![0x1B, 0x5B, 0x42]));
    }

    #[test]
    fn pty_bytes_arrow_right() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::ArrowRight), no_mods());
        assert_eq!(result, Some(vec![0x1B, 0x5B, 0x43]));
    }

    #[test]
    fn pty_bytes_arrow_left() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::ArrowLeft), no_mods());
        assert_eq!(result, Some(vec![0x1B, 0x5B, 0x44]));
    }

    // ================================================================
    // Unit 2, Test 5: Ctrl+C PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_ctrl_c() {
        let result = key_to_pty_bytes_from_key(&char_key("c"), ctrl_mods());
        assert_eq!(result, Some(vec![0x03]));
    }

    // ================================================================
    // Unit 2, Test 6: Tab PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_tab() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Tab), no_mods());
        assert_eq!(result, Some(vec![0x09]));
    }

    // ================================================================
    // Unit 2, Test 7: Escape PTY bytes
    // ================================================================

    #[test]
    fn pty_bytes_escape() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Escape), no_mods());
        assert_eq!(result, Some(vec![0x1B]));
    }

    // ================================================================
    // Unit 2, Test 8: Multi-byte UTF-8
    // ================================================================

    #[test]
    fn pty_bytes_multibyte_utf8() {
        let result = key_to_pty_bytes_from_key(&char_key("\u{00e9}"), no_mods());
        assert_eq!(result, Some(vec![0xC3, 0xA9]));
    }

    // ================================================================
    // Unit 2, Test 9: Logo modifier alone returns None from key_to_pty_bytes
    // ================================================================

    #[test]
    fn pty_bytes_logo_modifier_alone_returns_none() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Super), logo_mods());
        assert_eq!(result, None);
    }

    // ================================================================
    // Additional edge cases: more named keys
    // ================================================================

    #[test]
    fn pty_bytes_home() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Home), no_mods());
        assert_eq!(result, Some(vec![0x1B, b'[', b'H']));
    }

    #[test]
    fn pty_bytes_end() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::End), no_mods());
        assert_eq!(result, Some(vec![0x1B, b'[', b'F']));
    }

    #[test]
    fn pty_bytes_delete() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Delete), no_mods());
        assert_eq!(result, Some(vec![0x1B, b'[', b'3', b'~']));
    }

    #[test]
    fn pty_bytes_page_up() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::PageUp), no_mods());
        assert_eq!(result, Some(vec![0x1B, b'[', b'5', b'~']));
    }

    #[test]
    fn pty_bytes_page_down() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::PageDown), no_mods());
        assert_eq!(result, Some(vec![0x1B, b'[', b'6', b'~']));
    }

    // ================================================================
    // Additional Ctrl combos
    // ================================================================

    #[test]
    fn pty_bytes_ctrl_a() {
        let result = key_to_pty_bytes_from_key(&char_key("a"), ctrl_mods());
        assert_eq!(result, Some(vec![0x01]));
    }

    #[test]
    fn pty_bytes_ctrl_z() {
        let result = key_to_pty_bytes_from_key(&char_key("z"), ctrl_mods());
        assert_eq!(result, Some(vec![0x1A]));
    }

    // ================================================================
    // Space
    // ================================================================

    #[test]
    fn pty_bytes_space() {
        let result = key_to_pty_bytes_from_key(&named_key(NamedKey::Space), no_mods());
        assert_eq!(result, Some(vec![0x20]));
    }

    // ================================================================
    // Additional named key translations
    // ================================================================

    #[test]
    fn translate_named_key_tab() {
        let result = translate_logical_key(&named_key(NamedKey::Tab), no_mods());
        assert!(result.is_some(), "Tab should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Named("Tab".to_string()));
    }

    #[test]
    fn translate_named_key_escape() {
        let result = translate_logical_key(&named_key(NamedKey::Escape), no_mods());
        assert!(result.is_some(), "Escape should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Named("Escape".to_string()));
    }

    #[test]
    fn translate_named_key_backspace() {
        let result = translate_logical_key(&named_key(NamedKey::Backspace), no_mods());
        assert!(result.is_some(), "Backspace should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Named("Backspace".to_string()));
    }

    #[test]
    fn translate_named_key_arrow_up() {
        let result = translate_logical_key(&named_key(NamedKey::ArrowUp), no_mods());
        assert!(result.is_some(), "ArrowUp should translate");
        let input = result.unwrap();
        assert_eq!(input.key, keyboard::Key::Named("ArrowUp".to_string()));
    }

    // ================================================================
    // Case normalization with modifiers
    // ================================================================

    #[test]
    fn translate_character_key_uppercase_normalizes_with_logo() {
        let result = translate_logical_key(&char_key("D"), logo_mods());
        assert!(result.is_some());
        let input = result.unwrap();
        assert_eq!(
            input.key,
            keyboard::Key::Character('d'),
            "Character should be lowercased when logo modifier is held"
        );
    }

    #[test]
    fn translate_character_key_uppercase_normalizes_with_ctrl() {
        let result = translate_logical_key(&char_key("C"), ctrl_mods());
        assert!(result.is_some());
        let input = result.unwrap();
        assert_eq!(
            input.key,
            keyboard::Key::Character('c'),
            "Character should be lowercased when ctrl modifier is held"
        );
    }

    #[test]
    fn translate_character_key_no_normalize_without_modifier() {
        let result = translate_logical_key(&char_key("A"), shift_mods());
        assert!(result.is_some());
        let input = result.unwrap();
        assert_eq!(
            input.key,
            keyboard::Key::Character('A'),
            "Character should not be lowercased with only shift modifier"
        );
    }
}

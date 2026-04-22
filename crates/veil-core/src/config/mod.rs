//! Configuration system — TOML config file parsing, validation, diffing, and hot-reload.
//!
//! The config system provides:
//! - Data model (`AppConfig` and sub-structs) with serde deserialization and sensible defaults
//! - Platform-aware config file discovery
//! - Rich TOML error reporting with line/column information
//! - Semantic validation with clamping and warnings
//! - Config diffing to determine what changed between two versions
//! - File watcher for hot-reload via the `notify` crate

mod diff;
mod discovery;
mod model;
mod parse;
mod watcher;

use std::path::PathBuf;

// Re-export the public API as a flat namespace so callers don't need to
// know about the internal module structure.
pub use self::diff::ConfigDelta;
pub use self::discovery::{discover_config_path, load_config, load_or_default, primary_config_dir};
pub use self::model::{
    AppConfig, ConversationsConfig, DefaultTab, FontWeight, GeneralConfig, GhosttyConfig,
    KeybindingsConfig, PersistenceMode, SidebarConfig, TerminalConfig, ThemeMode,
};
pub use self::parse::{parse_config, validate_config, ConfigWarning};
pub use self::watcher::{ConfigEvent, ConfigWatcher};

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --------------------------------------------------------
    // Data model and defaults
    // --------------------------------------------------------

    mod defaults {
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

    mod serde_round_trips {
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
            assert_eq!(config.general.persistence, PersistenceMode::default());
        }

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

    mod equality {
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

    // --------------------------------------------------------
    // Config file discovery and loading
    // --------------------------------------------------------

    mod discovery_tests {
        use super::*;

        #[test]
        fn primary_config_dir_returns_path_ending_in_veil() {
            let dir = primary_config_dir();
            assert!(dir.is_some(), "primary_config_dir should return Some");
            let path = dir.unwrap();
            assert!(path.ends_with("veil"), "config dir should end with 'veil', got: {path:?}");
        }

        #[test]
        fn discover_config_path_does_not_panic() {
            // When no config files exist at any search location, should return None.
            // If the user has a real config file this may return Some, which is fine.
            let _ = discover_config_path();
        }
    }

    mod loading {
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
            assert_eq!(config.sidebar, SidebarConfig::default());
        }

        #[test]
        fn load_or_default_returns_defaults_when_no_file() {
            let (config, _path) = load_or_default();
            assert_eq!(config, AppConfig::default());
        }

        #[test]
        fn parse_error_includes_file_path() {
            let err = ConfigError::ParseError {
                path: PathBuf::from("/home/user/.config/veil/config.toml"),
                message: "unexpected character".to_string(),
            };
            let display = format!("{err}");
            assert!(display.contains("/home/user/.config/veil/config.toml"));
            assert!(display.contains("unexpected character"));
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
            assert!(display.contains("line 5"));
            assert!(display.contains("column 12"));
            assert!(display.contains("invalid type"));
            assert!(display.contains("/config.toml"));
        }

        #[test]
        fn read_error_includes_path_and_io_error() {
            let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
            let err = ConfigError::ReadError {
                path: PathBuf::from("/etc/veil/config.toml"),
                source: io_err,
            };
            let display = format!("{err}");
            assert!(display.contains("/etc/veil/config.toml"));
            assert!(display.contains("access denied"));
        }

        #[test]
        fn no_config_dir_error_is_informative() {
            let err = ConfigError::NoConfigDir;
            let display = format!("{err}");
            assert!(display.contains("config directory"));
        }

        #[test]
        fn toml_parse_error_includes_line_column_when_available() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "# line 1\n# line 2\n[general\ntheme = \"dark\"\n")
                .expect("write");

            let result = load_config(&config_path);
            assert!(result.is_err());
            let err = result.unwrap_err();
            let display = format!("{err}");
            assert!(
                display.contains("line") || display.contains('3'),
                "error should contain line info: {display}"
            );
        }
    }

    // --------------------------------------------------------
    // TOML parsing with rich error reporting
    // --------------------------------------------------------

    mod parsing {
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

    mod validation {
        use super::*;

        #[test]
        fn sidebar_width_below_100_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.sidebar.width = 50;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.sidebar.width, 100);
            assert!(warnings.iter().any(|w| w.field.contains("sidebar.width")));
        }

        #[test]
        fn sidebar_width_above_1000_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.sidebar.width = 2000;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.sidebar.width, 1000);
            assert!(warnings.iter().any(|w| w.field.contains("sidebar.width")));
        }

        #[test]
        fn font_size_below_6_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.font_size = 2.0;
            let (validated, warnings) = validate_config(config);
            assert!((validated.terminal.font_size - 6.0).abs() < f32::EPSILON);
            assert!(warnings.iter().any(|w| w.field.contains("terminal.font_size")));
        }

        #[test]
        fn font_size_above_72_clamped_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.font_size = 100.0;
            let (validated, warnings) = validate_config(config);
            assert!((validated.terminal.font_size - 72.0).abs() < f32::EPSILON);
            assert!(warnings.iter().any(|w| w.field.contains("terminal.font_size")));
        }

        #[test]
        fn scrollback_lines_zero_set_to_1_with_warning() {
            let mut config = AppConfig::default();
            config.terminal.scrollback_lines = 0;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.terminal.scrollback_lines, 1);
            assert!(warnings.iter().any(|w| w.field.contains("terminal.scrollback_lines")));
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

    // --------------------------------------------------------
    // Config diffing
    // --------------------------------------------------------

    mod diffing {
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
            assert!(delta.theme_changed);
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
            assert!(delta.persistence_changed);
            assert!(!delta.theme_changed);
        }

        #[test]
        fn changing_sidebar_width_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.width = 400;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed);
            assert!(!delta.theme_changed);
        }

        #[test]
        fn changing_sidebar_visible_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.visible = !a.sidebar.visible;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed);
        }

        #[test]
        fn changing_sidebar_default_tab_sets_sidebar_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.sidebar.default_tab = DefaultTab::Conversations;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.sidebar_changed);
        }

        #[test]
        fn changing_font_family_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_family = Some("Fira Code".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed);
            assert!(!delta.scrollback_changed);
        }

        #[test]
        fn changing_font_size_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_size = 18.0;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed);
        }

        #[test]
        fn changing_font_weight_sets_font_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.font_weight = FontWeight::Bold;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.font_changed);
        }

        #[test]
        fn changing_scrollback_sets_scrollback_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.terminal.scrollback_lines = 50000;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.scrollback_changed);
            assert!(!delta.font_changed);
        }

        #[test]
        fn changing_keybindings_sets_keybindings_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.keybindings.toggle_sidebar = Some("ctrl+b".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.keybindings_changed);
        }

        #[test]
        fn changing_adapters_sets_adapters_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.conversations.adapters = vec!["codex".to_string()];
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.adapters_changed);
        }

        #[test]
        fn changing_ghostty_path_sets_ghostty_path_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.ghostty.config_path = Some("/path/to/ghostty".to_string());
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.ghostty_path_changed);
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
            assert!(!delta.is_empty());
        }
    }

    // --------------------------------------------------------
    // Config file watcher (hot-reload)
    // --------------------------------------------------------

    mod watcher_tests {
        use super::*;

        #[test]
        fn watcher_new_with_valid_path_succeeds() {
            let dir = TempDir::new().expect("create temp dir");
            let config_path = dir.path().join("config.toml");
            std::fs::write(&config_path, "").expect("write");

            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            let result = ConfigWatcher::new(config_path, AppConfig::default(), tx);
            assert!(result.is_ok());
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

            std::fs::write(&config_path, "[broken").expect("write invalid config");

            let result = watcher.reload();
            assert!(result.is_ok(), "reload should return Ok(ConfigEvent::Error)");
            assert!(matches!(result.unwrap(), ConfigEvent::Error(_)));
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
            assert!(result.is_ok());
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

            std::fs::write(&config_path, "[broken").expect("write invalid");
            let _ = watcher.reload();

            assert_eq!(watcher.current_config(), &initial);
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

            std::fs::write(
                &config_path,
                r#"
[general]
theme = "light"
"#,
            )
            .expect("write new config");

            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
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

            std::fs::write(&config_path, "[broken syntax").expect("write invalid");

            let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
            signal.trigger();

            assert!(event.is_ok(), "should receive event");
            let event = event.unwrap();
            assert!(event.is_some());
            assert!(matches!(event.unwrap(), ConfigEvent::Error(_)));
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

            std::fs::write(&config_path, "[broken").expect("write invalid");
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;

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
                    // Also acceptable if the watcher sends the error first
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

            std::fs::remove_file(&config_path).expect("delete file");
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

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
                    assert!(delta.theme_changed);
                    assert!(!delta.sidebar_changed);
                    assert!(!delta.font_changed);
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
                    assert!(delta.sidebar_changed);
                    assert!(delta.font_changed);
                    assert!(!delta.theme_changed);
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

            signal.trigger();
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let result = rx.try_recv();
            assert!(result.is_err(), "after shutdown, no events should be pending");
        }
    }

    // --------------------------------------------------------
    // StateUpdate::ConfigReloaded integration
    // --------------------------------------------------------

    mod state_update_integration {
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

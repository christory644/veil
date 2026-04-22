//! TOML parsing with rich error reporting and semantic validation.

use std::path::Path;

use super::{AppConfig, ConfigError};

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
                let (line, column) = byte_offset_to_line_col(toml_str, span.start);
                ConfigError::ParseErrorWithLocation {
                    path: source_path.to_path_buf(),
                    line,
                    column,
                    message,
                }
            }
            None => ConfigError::ParseError { path: source_path.to_path_buf(), message },
        }
    })
}

/// Convert a byte offset in a string to a 1-based (line, column) pair.
fn byte_offset_to_line_col(s: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in s.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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

    clamp_u32(&mut config.sidebar.width, 100, 1000, "sidebar.width", &mut warnings);
    clamp_f32(&mut config.terminal.font_size, 6.0, 72.0, "terminal.font_size", &mut warnings);

    if config.terminal.scrollback_lines == 0 {
        warnings.push(ConfigWarning {
            field: "terminal.scrollback_lines".to_string(),
            message: "terminal.scrollback_lines is 0, set to 1".to_string(),
        });
        config.terminal.scrollback_lines = 1;
    }

    (config, warnings)
}

/// Clamp a `u32` field to `[min, max]`, pushing a warning if out of range.
fn clamp_u32(value: &mut u32, min: u32, max: u32, field: &str, warnings: &mut Vec<ConfigWarning>) {
    if *value < min {
        warnings.push(ConfigWarning {
            field: field.to_string(),
            message: format!("{field} {} is below minimum {min}, clamped to {min}", *value),
        });
        *value = min;
    } else if *value > max {
        warnings.push(ConfigWarning {
            field: field.to_string(),
            message: format!("{field} {} is above maximum {max}, clamped to {max}", *value),
        });
        *value = max;
    }
}

/// Clamp an `f32` field to `[min, max]`, pushing a warning if out of range.
fn clamp_f32(value: &mut f32, min: f32, max: f32, field: &str, warnings: &mut Vec<ConfigWarning>) {
    if *value < min {
        warnings.push(ConfigWarning {
            field: field.to_string(),
            message: format!("{field} {} is below minimum {min}, clamped to {min}", *value),
        });
        *value = min;
    } else if *value > max {
        warnings.push(ConfigWarning {
            field: field.to_string(),
            message: format!("{field} {} is above maximum {max}, clamped to {max}", *value),
        });
        *value = max;
    }
}

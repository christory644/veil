//! Shared types for the PTY abstraction.

use std::path::PathBuf;

/// Terminal dimensions in cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of columns (characters).
    pub cols: u16,
    /// Number of rows (characters).
    pub rows: u16,
    /// Width in pixels (optional, used by some applications).
    pub pixel_width: u16,
    /// Height in pixels (optional, used by some applications).
    pub pixel_height: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self { cols: 80, rows: 24, pixel_width: 0, pixel_height: 0 }
    }
}

/// Configuration for spawning a new PTY.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    /// Command to execute (e.g., "/bin/zsh"). If `None`, uses `$SHELL` or `/bin/sh`.
    pub command: Option<String>,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Working directory. Defaults to `$HOME` if `None`.
    pub working_directory: Option<PathBuf>,
    /// Additional environment variables to set (key, value).
    /// These are added on top of the inherited environment.
    pub env: Vec<(String, String)>,
    /// Initial terminal size.
    pub size: PtySize,
}

/// Events emitted by the PTY read loop.
#[derive(Debug)]
pub enum PtyEvent {
    /// Output bytes read from the PTY master fd.
    Output(Vec<u8>),
    /// The child process exited.
    ChildExited {
        /// Exit code if the child exited normally.
        exit_code: Option<i32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PtySize ---

    #[test]
    fn pty_size_construction_and_field_access() {
        let size = PtySize { cols: 120, rows: 40, pixel_width: 960, pixel_height: 640 };
        assert_eq!(size.cols, 120);
        assert_eq!(size.rows, 40);
        assert_eq!(size.pixel_width, 960);
        assert_eq!(size.pixel_height, 640);
    }

    #[test]
    fn pty_size_default_is_80x24_zero_pixels() {
        let size = PtySize::default();
        assert_eq!(size.cols, 80);
        assert_eq!(size.rows, 24);
        assert_eq!(size.pixel_width, 0);
        assert_eq!(size.pixel_height, 0);
    }

    #[test]
    fn pty_size_clone_produces_equal_copy() {
        let size = PtySize { cols: 132, rows: 43, pixel_width: 100, pixel_height: 200 };
        let cloned = size;
        assert_eq!(size, cloned);
    }

    #[test]
    fn pty_size_debug_format_is_readable() {
        let size = PtySize::default();
        let debug = format!("{size:?}");
        assert!(debug.contains("cols"));
        assert!(debug.contains("rows"));
    }

    // --- PtyConfig ---

    #[test]
    fn pty_config_with_all_fields_populated() {
        let config = PtyConfig {
            command: Some("/bin/zsh".to_string()),
            args: vec!["-l".to_string()],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![
                ("FOO".to_string(), "bar".to_string()),
                ("BAZ".to_string(), "qux".to_string()),
            ],
            size: PtySize { cols: 120, rows: 40, pixel_width: 0, pixel_height: 0 },
        };
        assert_eq!(config.command.as_deref(), Some("/bin/zsh"));
        assert_eq!(config.args.len(), 1);
        assert_eq!(config.working_directory, Some(PathBuf::from("/tmp")));
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.size.cols, 120);
    }

    #[test]
    fn pty_config_with_none_command_for_default_shell() {
        let config = PtyConfig {
            command: None,
            args: vec![],
            working_directory: None,
            env: vec![],
            size: PtySize::default(),
        };
        assert!(config.command.is_none());
        assert!(config.working_directory.is_none());
    }

    #[test]
    fn pty_config_with_empty_env_vec() {
        let config = PtyConfig {
            command: Some("/bin/sh".to_string()),
            args: vec![],
            working_directory: None,
            env: vec![],
            size: PtySize::default(),
        };
        assert!(config.env.is_empty());
    }

    #[test]
    fn pty_config_with_multiple_env_entries() {
        let config = PtyConfig {
            command: None,
            args: vec![],
            working_directory: None,
            env: vec![
                ("TERM".to_string(), "xterm-ghostty".to_string()),
                ("TERM_PROGRAM".to_string(), "ghostty".to_string()),
                ("VEIL_SURFACE_ID".to_string(), "42".to_string()),
            ],
            size: PtySize::default(),
        };
        assert_eq!(config.env.len(), 3);
        assert_eq!(config.env[0].0, "TERM");
        assert_eq!(config.env[2].1, "42");
    }

    #[test]
    fn pty_config_clone_preserves_all_fields() {
        let config = PtyConfig {
            command: Some("/bin/zsh".to_string()),
            args: vec!["-l".to_string()],
            working_directory: Some(PathBuf::from("/home/user")),
            env: vec![("KEY".to_string(), "VALUE".to_string())],
            size: PtySize { cols: 100, rows: 50, pixel_width: 800, pixel_height: 600 },
        };
        let cloned = config.clone();
        assert_eq!(cloned.command, config.command);
        assert_eq!(cloned.args, config.args);
        assert_eq!(cloned.working_directory, config.working_directory);
        assert_eq!(cloned.env, config.env);
        assert_eq!(cloned.size, config.size);
    }

    // --- PtyEvent ---

    #[test]
    fn pty_event_output_holds_arbitrary_bytes_including_nul() {
        let data = vec![0x00, 0x01, 0xFF, b'h', b'e', b'l', b'l', b'o', 0x00];
        let event = PtyEvent::Output(data.clone());
        match event {
            PtyEvent::Output(bytes) => assert_eq!(bytes, data),
            PtyEvent::ChildExited { .. } => panic!("expected Output variant"),
        }
    }

    #[test]
    fn pty_event_child_exited_with_some_exit_code() {
        let event = PtyEvent::ChildExited { exit_code: Some(42) };
        match event {
            PtyEvent::ChildExited { exit_code } => assert_eq!(exit_code, Some(42)),
            PtyEvent::Output(_) => panic!("expected ChildExited variant"),
        }
    }

    #[test]
    fn pty_event_child_exited_with_none_exit_code() {
        let event = PtyEvent::ChildExited { exit_code: None };
        match event {
            PtyEvent::ChildExited { exit_code } => assert!(exit_code.is_none()),
            PtyEvent::Output(_) => panic!("expected ChildExited variant"),
        }
    }

    #[test]
    fn pty_event_output_empty_vec() {
        let event = PtyEvent::Output(vec![]);
        match event {
            PtyEvent::Output(bytes) => assert!(bytes.is_empty()),
            PtyEvent::ChildExited { .. } => panic!("expected Output variant"),
        }
    }

    #[test]
    fn pty_event_debug_format_exists() {
        let event = PtyEvent::Output(vec![1, 2, 3]);
        let debug = format!("{event:?}");
        assert!(debug.contains("Output"));
    }
}

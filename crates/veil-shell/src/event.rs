//! Shell event types.
//!
//! The typed events that `veil-shell` produces, consumed by the PTY I/O
//! integration layer (VEI-70).

use std::path::PathBuf;

use veil_core::session::AgentKind;
use veil_core::workspace::SurfaceId;

use crate::agent_detector::DetectedAgent;
use crate::env_detector::EnvReport;

/// Events produced by shell integration processing.
///
/// These are emitted by the [`crate::tracker::SurfaceShellState`] tracker
/// when it observes meaningful state changes. The consumer (PTY I/O loop,
/// event loop) maps these to `StateUpdate` messages or direct `AppState`
/// mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// The shell's working directory changed (OSC 7 received).
    DirectoryChanged {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The new working directory.
        path: PathBuf,
    },

    /// An AI agent process was detected in the surface's process tree.
    AgentStarted {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The detected agent.
        agent: DetectedAgent,
    },

    /// A previously detected AI agent process is no longer running.
    AgentStopped {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// Which agent kind stopped.
        agent_kind: AgentKind,
        /// The PID that was previously detected.
        pid: u32,
    },

    /// The environment characteristics of the working directory changed.
    EnvironmentChanged {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The new environment report.
        env: EnvReport,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_detector::EnvKind;

    fn test_surface_id() -> SurfaceId {
        SurfaceId::new(1)
    }

    // ── All variants are constructible ──────────────────────────────

    #[test]
    fn construct_directory_changed() {
        let event = ShellEvent::DirectoryChanged {
            surface_id: test_surface_id(),
            path: PathBuf::from("/home/user/project"),
        };
        assert!(matches!(event, ShellEvent::DirectoryChanged { .. }));
    }

    #[test]
    fn construct_agent_started() {
        let event = ShellEvent::AgentStarted {
            surface_id: test_surface_id(),
            agent: DetectedAgent {
                kind: AgentKind::ClaudeCode,
                pid: 42,
                exe_name: "claude".to_string(),
            },
        };
        assert!(matches!(event, ShellEvent::AgentStarted { .. }));
    }

    #[test]
    fn construct_agent_stopped() {
        let event = ShellEvent::AgentStopped {
            surface_id: test_surface_id(),
            agent_kind: AgentKind::Codex,
            pid: 99,
        };
        assert!(matches!(event, ShellEvent::AgentStopped { .. }));
    }

    #[test]
    fn construct_environment_changed() {
        let event = ShellEvent::EnvironmentChanged {
            surface_id: test_surface_id(),
            env: EnvReport { kinds: vec![EnvKind::Git, EnvKind::Rust] },
        };
        assert!(matches!(event, ShellEvent::EnvironmentChanged { .. }));
    }

    // ── Trait implementations ───────────────────────────────────────

    #[test]
    fn events_are_cloneable() {
        let event = ShellEvent::DirectoryChanged {
            surface_id: test_surface_id(),
            path: PathBuf::from("/tmp"),
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[test]
    fn events_are_debug_printable() {
        let event = ShellEvent::AgentStarted {
            surface_id: test_surface_id(),
            agent: DetectedAgent { kind: AgentKind::Aider, pid: 7, exe_name: "aider".to_string() },
        };
        let debug = format!("{event:?}");
        assert!(debug.contains("AgentStarted"));
        assert!(debug.contains("aider"));
    }

    #[test]
    fn events_are_comparable() {
        let event1 = ShellEvent::DirectoryChanged {
            surface_id: SurfaceId::new(1),
            path: PathBuf::from("/a"),
        };
        let event2 = ShellEvent::DirectoryChanged {
            surface_id: SurfaceId::new(1),
            path: PathBuf::from("/a"),
        };
        let event3 = ShellEvent::DirectoryChanged {
            surface_id: SurfaceId::new(2),
            path: PathBuf::from("/a"),
        };
        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }

    // ── Pattern matching ────────────────────────────────────────────

    #[test]
    fn pattern_match_all_variants() {
        let events = vec![
            ShellEvent::DirectoryChanged {
                surface_id: test_surface_id(),
                path: PathBuf::from("/tmp"),
            },
            ShellEvent::AgentStarted {
                surface_id: test_surface_id(),
                agent: DetectedAgent {
                    kind: AgentKind::ClaudeCode,
                    pid: 1,
                    exe_name: "claude".to_string(),
                },
            },
            ShellEvent::AgentStopped {
                surface_id: test_surface_id(),
                agent_kind: AgentKind::ClaudeCode,
                pid: 1,
            },
            ShellEvent::EnvironmentChanged {
                surface_id: test_surface_id(),
                env: EnvReport { kinds: vec![] },
            },
        ];

        let mut dir_count = 0;
        let mut start_count = 0;
        let mut stop_count = 0;
        let mut env_count = 0;

        for event in &events {
            match event {
                ShellEvent::DirectoryChanged { .. } => dir_count += 1,
                ShellEvent::AgentStarted { .. } => start_count += 1,
                ShellEvent::AgentStopped { .. } => stop_count += 1,
                ShellEvent::EnvironmentChanged { .. } => env_count += 1,
            }
        }

        assert_eq!(dir_count, 1);
        assert_eq!(start_count, 1);
        assert_eq!(stop_count, 1);
        assert_eq!(env_count, 1);
    }
}

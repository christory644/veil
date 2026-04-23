//! Surface state tracker.
//!
//! Per-surface state machine that tracks the current directory, active agent,
//! and environment. Deduplicates redundant events (e.g., repeated OSC 7 with
//! the same path).

use std::path::PathBuf;

use veil_core::workspace::SurfaceId;

use crate::agent_detector::{detect_agents, DetectedAgent, ProcessEntry};
use crate::env_detector::{detect_env, EnvReport};
use crate::event::ShellEvent;
use crate::osc7::parse_osc7;

/// Tracked state for a single terminal surface.
#[derive(Debug)]
pub struct SurfaceShellState {
    /// Which surface this tracks.
    surface_id: SurfaceId,
    /// Current working directory (last OSC 7 value).
    current_dir: Option<PathBuf>,
    /// Currently detected agent (most recently detected).
    active_agent: Option<DetectedAgent>,
    /// Current environment report.
    current_env: Option<EnvReport>,
}

impl SurfaceShellState {
    /// Create a new tracker for a surface.
    pub fn new(surface_id: SurfaceId) -> Self {
        Self { surface_id, current_dir: None, active_agent: None, current_env: None }
    }

    /// Get the current working directory, if known.
    pub fn current_dir(&self) -> Option<&PathBuf> {
        self.current_dir.as_ref()
    }

    /// Get the currently active agent, if any.
    pub fn active_agent(&self) -> Option<&DetectedAgent> {
        self.active_agent.as_ref()
    }

    /// Get the current environment report, if any.
    pub fn current_env(&self) -> Option<&EnvReport> {
        self.current_env.as_ref()
    }

    /// Process an OSC 7 payload. Returns a `ShellEvent::DirectoryChanged`
    /// if the directory actually changed, or `None` if it is the same as
    /// the current directory. Also triggers environment re-detection on
    /// directory change.
    ///
    /// Returns up to 2 events: `DirectoryChanged` and `EnvironmentChanged`.
    pub fn handle_osc7(&mut self, payload: &str) -> Vec<ShellEvent> {
        let Ok(report) = parse_osc7(payload) else {
            return vec![];
        };

        let new_path = report.path;

        // Dedup: if the path is the same as the current one, no events.
        if self.current_dir.as_ref() == Some(&new_path) {
            return vec![];
        }

        // Path changed (or first time).
        self.current_dir = Some(new_path.clone());
        let mut events = vec![ShellEvent::DirectoryChanged {
            surface_id: self.surface_id,
            path: new_path.clone(),
        }];

        // Cascade: detect environment for the new directory.
        let new_env = detect_env(&new_path);
        if self.current_env.as_ref() != Some(&new_env) {
            self.current_env = Some(new_env.clone());
            events
                .push(ShellEvent::EnvironmentChanged { surface_id: self.surface_id, env: new_env });
        }

        events
    }

    /// Update agent detection from a process list snapshot.
    ///
    /// Compares the current process tree against the previously detected
    /// agent. Emits `AgentStarted` if a new agent is found, `AgentStopped`
    /// if the previous agent is gone, or both if the agent changed.
    pub fn update_agents(&mut self, root_pid: u32, processes: &[ProcessEntry]) -> Vec<ShellEvent> {
        let detected = detect_agents(root_pid, processes);
        let primary = detected.into_iter().next();

        let mut events = Vec::new();

        match (&self.active_agent, &primary) {
            // No previous, no current -> nothing.
            (None, None) => {}
            // No previous, agent detected -> AgentStarted.
            (None, Some(agent)) => {
                events.push(ShellEvent::AgentStarted {
                    surface_id: self.surface_id,
                    agent: agent.clone(),
                });
                self.active_agent = Some(agent.clone());
            }
            // Previous agent, same agent still running -> dedup.
            (Some(prev), Some(curr)) if prev.kind == curr.kind && prev.pid == curr.pid => {}
            // Previous agent, different agent detected -> stop old, start new.
            (Some(prev), Some(curr)) => {
                events.push(ShellEvent::AgentStopped {
                    surface_id: self.surface_id,
                    agent_kind: prev.kind.clone(),
                    pid: prev.pid,
                });
                events.push(ShellEvent::AgentStarted {
                    surface_id: self.surface_id,
                    agent: curr.clone(),
                });
                self.active_agent = Some(curr.clone());
            }
            // Previous agent, no longer detected -> AgentStopped.
            (Some(prev), None) => {
                events.push(ShellEvent::AgentStopped {
                    surface_id: self.surface_id,
                    agent_kind: prev.kind.clone(),
                    pid: prev.pid,
                });
                self.active_agent = None;
            }
        }

        events
    }

    /// Force re-detection of the environment for the current directory.
    /// Returns `EnvironmentChanged` if the environment differs from the
    /// previously reported one, or an empty vec if unchanged.
    pub fn refresh_env(&mut self) -> Vec<ShellEvent> {
        let Some(dir) = &self.current_dir else {
            return vec![];
        };

        let new_env = detect_env(dir);
        if self.current_env.as_ref() == Some(&new_env) {
            vec![]
        } else {
            self.current_env = Some(new_env.clone());
            vec![ShellEvent::EnvironmentChanged { surface_id: self.surface_id, env: new_env }]
        }
    }

    /// Reset all tracked state (e.g., when the surface's process exits).
    pub fn reset(&mut self) {
        self.current_dir = None;
        self.active_agent = None;
        self.current_env = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_detector::ProcessEntry;
    use veil_core::session::AgentKind;

    fn sid(n: u64) -> SurfaceId {
        SurfaceId::new(n)
    }

    #[allow(clippy::similar_names)]
    fn proc(pid: u32, ppid: u32, name: &str) -> ProcessEntry {
        ProcessEntry { pid, ppid, name: name.to_string() }
    }

    // ── Directory tracking ──────────────────────────────────────────

    #[test]
    fn handle_osc7_emits_directory_changed() {
        let mut state = SurfaceShellState::new(sid(1));
        let events = state.handle_osc7("7;file:///home/user/project");
        assert!(
            events.iter().any(|e| matches!(
                e,
                ShellEvent::DirectoryChanged { path, .. }
                if path.as_path() == std::path::Path::new("/home/user/project")
            )),
            "expected DirectoryChanged event, got: {events:?}"
        );
    }

    #[test]
    fn handle_osc7_deduplicates_same_path() {
        let mut state = SurfaceShellState::new(sid(1));
        let _first = state.handle_osc7("7;file:///home/user/project");
        let second = state.handle_osc7("7;file:///home/user/project");
        assert!(
            !second.iter().any(|e| matches!(e, ShellEvent::DirectoryChanged { .. })),
            "should not emit DirectoryChanged for same path, got: {second:?}"
        );
    }

    #[test]
    fn handle_osc7_emits_on_different_path() {
        let mut state = SurfaceShellState::new(sid(1));
        let _first = state.handle_osc7("7;file:///home/user/project-a");
        let second = state.handle_osc7("7;file:///home/user/project-b");
        assert!(
            second.iter().any(|e| matches!(
                e,
                ShellEvent::DirectoryChanged { path, .. }
                if path.as_path() == std::path::Path::new("/home/user/project-b")
            )),
            "expected DirectoryChanged for new path, got: {second:?}"
        );
    }

    #[test]
    fn handle_osc7_invalid_payload_emits_nothing() {
        let mut state = SurfaceShellState::new(sid(1));
        let events = state.handle_osc7("0;window title");
        assert!(events.is_empty(), "invalid OSC payload should produce no events");
    }

    #[test]
    fn current_dir_returns_last_valid_directory() {
        let mut state = SurfaceShellState::new(sid(1));
        assert!(state.current_dir().is_none());
        state.handle_osc7("7;file:///tmp/test");
        assert_eq!(state.current_dir(), Some(&PathBuf::from("/tmp/test")));
    }

    // ── Environment cascading ───────────────────────────────────────

    #[test]
    fn directory_change_triggers_env_detection() {
        // Use a tempdir with known markers so env detection finds something.
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();

        let uri = format!("7;file://{}", tmp.path().display());
        let mut state = SurfaceShellState::new(sid(1));
        let events = state.handle_osc7(&uri);

        // Should have both DirectoryChanged and EnvironmentChanged
        assert!(
            events.iter().any(|e| matches!(e, ShellEvent::DirectoryChanged { .. })),
            "expected DirectoryChanged, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, ShellEvent::EnvironmentChanged { .. })),
            "expected EnvironmentChanged, got: {events:?}"
        );
    }

    #[test]
    fn same_env_not_re_emitted_on_dir_change() {
        // Two directories with the same env markers -- second change should
        // emit DirectoryChanged but NOT EnvironmentChanged.
        let tmp1 = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp1.path().join("Cargo.toml"), "").unwrap();

        let tmp2 = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp2.path().join("Cargo.toml"), "").unwrap();

        let mut state = SurfaceShellState::new(sid(1));
        let _first = state.handle_osc7(&format!("7;file://{}", tmp1.path().display()));
        let second = state.handle_osc7(&format!("7;file://{}", tmp2.path().display()));

        assert!(
            second.iter().any(|e| matches!(e, ShellEvent::DirectoryChanged { .. })),
            "expected DirectoryChanged"
        );
        assert!(
            !second.iter().any(|e| matches!(e, ShellEvent::EnvironmentChanged { .. })),
            "should NOT emit EnvironmentChanged when env is same as previous"
        );
    }

    #[test]
    fn refresh_env_with_no_current_dir_returns_empty() {
        let mut state = SurfaceShellState::new(sid(1));
        let events = state.refresh_env();
        assert!(events.is_empty());
    }

    // ── Agent tracking ──────────────────────────────────────────────

    #[test]
    fn update_agents_detects_new_agent() {
        let mut state = SurfaceShellState::new(sid(1));
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        let events = state.update_agents(100, &procs);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ShellEvent::AgentStarted { agent, .. }
                if agent.kind == AgentKind::ClaudeCode
            )),
            "expected AgentStarted, got: {events:?}"
        );
        assert!(state.active_agent().is_some());
    }

    #[test]
    fn update_agents_deduplicates_same_agent() {
        let mut state = SurfaceShellState::new(sid(1));
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        let _first = state.update_agents(100, &procs);
        let second = state.update_agents(100, &procs);
        assert!(
            second.is_empty(),
            "should not emit events for same agent still running, got: {second:?}"
        );
    }

    #[test]
    fn update_agents_detects_agent_stopped() {
        let mut state = SurfaceShellState::new(sid(1));
        let procs_with_agent = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        let _started = state.update_agents(100, &procs_with_agent);

        let procs_without_agent = vec![proc(100, 1, "zsh")];
        let events = state.update_agents(100, &procs_without_agent);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ShellEvent::AgentStopped { agent_kind, pid, .. }
                if *agent_kind == AgentKind::ClaudeCode && *pid == 200
            )),
            "expected AgentStopped, got: {events:?}"
        );
        assert!(state.active_agent().is_none());
    }

    #[test]
    fn update_agents_handles_agent_change() {
        let mut state = SurfaceShellState::new(sid(1));
        let procs_claude = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        let _started = state.update_agents(100, &procs_claude);

        let procs_aider = vec![proc(100, 1, "zsh"), proc(300, 100, "aider")];
        let events = state.update_agents(100, &procs_aider);

        // Should have both a stop for claude and a start for aider
        assert!(
            events.iter().any(|e| matches!(
                e,
                ShellEvent::AgentStopped { agent_kind, .. }
                if *agent_kind == AgentKind::ClaudeCode
            )),
            "expected AgentStopped for claude, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ShellEvent::AgentStarted { agent, .. }
                if agent.kind == AgentKind::Aider
            )),
            "expected AgentStarted for aider, got: {events:?}"
        );
    }

    #[test]
    fn update_agents_no_event_when_none_active_and_none_found() {
        let mut state = SurfaceShellState::new(sid(1));
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "vim")];
        let events = state.update_agents(100, &procs);
        assert!(events.is_empty());
    }

    // ── Reset ───────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_state() {
        let mut state = SurfaceShellState::new(sid(1));
        state.handle_osc7("7;file:///tmp/test");
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        state.update_agents(100, &procs);

        state.reset();

        assert!(state.current_dir().is_none());
        assert!(state.active_agent().is_none());
        assert!(state.current_env().is_none());
    }

    #[test]
    fn reset_allows_re_detection_of_same_path() {
        let mut state = SurfaceShellState::new(sid(1));
        let _first = state.handle_osc7("7;file:///tmp/test");
        state.reset();
        let events = state.handle_osc7("7;file:///tmp/test");
        assert!(
            events.iter().any(|e| matches!(e, ShellEvent::DirectoryChanged { .. })),
            "after reset, same path should emit DirectoryChanged again, got: {events:?}"
        );
    }

    // ── Property-based ──────────────────────────────────────────────

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn handle_osc7_emits_at_most_one_directory_changed(
                path1 in "/[a-z]{1,20}",
                path2 in "/[a-z]{1,20}",
            ) {
                let mut state = SurfaceShellState::new(sid(1));
                let events = state.handle_osc7(&format!("7;file://{path1}"));
                let dir_changed_count = events.iter()
                    .filter(|e| matches!(e, ShellEvent::DirectoryChanged { .. }))
                    .count();
                prop_assert!(dir_changed_count <= 1, "at most 1 DirectoryChanged per call");

                let events2 = state.handle_osc7(&format!("7;file://{path2}"));
                let dir_changed_count2 = events2.iter()
                    .filter(|e| matches!(e, ShellEvent::DirectoryChanged { .. }))
                    .count();
                prop_assert!(dir_changed_count2 <= 1, "at most 1 DirectoryChanged per call");
            }

            #[test]
            fn update_agents_never_starts_and_stops_same_kind(
                agent_name in "(claude|codex|opencode|aider|vim|bash)",
            ) {
                let mut state = SurfaceShellState::new(sid(1));
                let procs = vec![
                    proc(100, 1, "zsh"),
                    proc(200, 100, &agent_name),
                ];
                let events = state.update_agents(100, &procs);

                // Collect started and stopped kinds
                let started: Vec<_> = events.iter().filter_map(|e| {
                    if let ShellEvent::AgentStarted { agent, .. } = e {
                        Some(agent.kind.clone())
                    } else {
                        None
                    }
                }).collect();
                let stopped: Vec<_> = events.iter().filter_map(|e| {
                    if let ShellEvent::AgentStopped { agent_kind, .. } = e {
                        Some(agent_kind.clone())
                    } else {
                        None
                    }
                }).collect();

                // Should never start AND stop the same kind in one call
                for kind in &started {
                    prop_assert!(
                        !stopped.contains(kind),
                        "should not start and stop same kind in one call"
                    );
                }
            }
        }
    }
}

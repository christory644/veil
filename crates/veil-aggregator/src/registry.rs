//! Adapter registry that coordinates discovery across multiple agent adapters.
//!
//! The registry holds all registered [`AgentAdapter`] implementations and provides
//! a unified interface for session discovery, preview lookup, and watch path collection.

use std::path::PathBuf;
use tracing::warn;
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionPreview};

use crate::adapter::AgentAdapter;

/// Manages a collection of agent adapters and coordinates operations across them.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self { adapters: Vec::new() }
    }

    /// Register an adapter.
    pub fn register(&mut self, adapter: Box<dyn AgentAdapter>) {
        self.adapters.push(adapter);
    }

    /// Discover sessions from all registered adapters.
    ///
    /// Failed adapters are skipped with a warning log; others continue.
    /// Returns all successfully discovered sessions.
    pub fn discover_all(&self) -> Vec<SessionEntry> {
        let mut entries = Vec::new();
        for adapter in &self.adapters {
            for result in adapter.discover_sessions() {
                match result {
                    Ok(entry) => entries.push(entry),
                    Err(err) => {
                        warn!(
                            adapter = adapter.name(),
                            %err,
                            "skipping session that failed to load from adapter"
                        );
                    }
                }
            }
        }
        entries
    }

    /// Get preview from the appropriate adapter for a given session.
    ///
    /// Adapter errors are logged and treated as "not found" (returns `None`),
    /// consistent with how [`discover_all`](Self::discover_all) handles failures.
    pub fn session_preview(&self, agent: &AgentKind, id: &SessionId) -> Option<SessionPreview> {
        self.adapters.iter().find(|a| a.agent_kind() == *agent).and_then(|a| {
            match a.session_preview(id) {
                Ok(preview) => preview,
                Err(err) => {
                    warn!(
                        adapter = a.name(),
                        %err,
                        "failed to load session preview"
                    );
                    None
                }
            }
        })
    }

    /// Collect all watch paths from all adapters.
    pub fn all_watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for adapter in &self.adapters {
            for path in adapter.watch_paths() {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    /// List registered adapter names.
    pub fn adapter_names(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.name()).collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::AdapterError;
    use crate::testutil::{make_test_entry, MockAdapter};

    #[test]
    fn empty_registry_discover_all_returns_empty() {
        let registry = AdapterRegistry::new();
        let sessions = registry.discover_all();
        assert!(sessions.is_empty());
    }

    #[test]
    fn single_adapter_returns_its_sessions() {
        let mut registry = AdapterRegistry::new();
        let adapter = MockAdapter::new("Claude Code", AgentKind::ClaudeCode).with_sessions(vec![
            make_test_entry("s1", AgentKind::ClaudeCode),
            make_test_entry("s2", AgentKind::ClaudeCode),
        ]);
        registry.register(Box::new(adapter));

        let sessions = registry.discover_all();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn multiple_adapters_return_combined_sessions() {
        let mut registry = AdapterRegistry::new();

        let adapter1 = MockAdapter::new("Claude Code", AgentKind::ClaudeCode)
            .with_sessions(vec![make_test_entry("cc1", AgentKind::ClaudeCode)]);
        let adapter2 = MockAdapter::new("Codex", AgentKind::Codex)
            .with_sessions(vec![make_test_entry("cx1", AgentKind::Codex)]);

        registry.register(Box::new(adapter1));
        registry.register(Box::new(adapter2));

        let sessions = registry.discover_all();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn adapter_errors_logged_but_valid_entries_still_returned() {
        let mut registry = AdapterRegistry::new();

        // Adapter with a mix of good sessions and errors
        let adapter_with_errors = MockAdapter::new("Buggy", AgentKind::ClaudeCode)
            .with_sessions(vec![make_test_entry("good1", AgentKind::ClaudeCode)])
            .with_errors(vec![AdapterError::DataDirNotFound(PathBuf::from("/missing"))]);

        // A healthy adapter
        let healthy_adapter = MockAdapter::new("Codex", AgentKind::Codex)
            .with_sessions(vec![make_test_entry("cx1", AgentKind::Codex)]);

        registry.register(Box::new(adapter_with_errors));
        registry.register(Box::new(healthy_adapter));

        let sessions = registry.discover_all();
        // Should contain good sessions from both adapters, errors skipped
        assert!(
            sessions.len() >= 2,
            "should have at least 2 valid sessions, got {}",
            sessions.len()
        );
    }

    #[test]
    fn session_preview_routes_to_correct_adapter() {
        let mut registry = AdapterRegistry::new();

        let cc_preview = SessionPreview {
            id: SessionId::new("cc-preview"),
            first_user_message: Some("CC message".to_string()),
            first_assistant_message: None,
            message_count: 5,
            tool_call_count: 2,
        };
        let adapter =
            MockAdapter::new("Claude Code", AgentKind::ClaudeCode).with_preview(cc_preview);
        registry.register(Box::new(adapter));

        let preview =
            registry.session_preview(&AgentKind::ClaudeCode, &SessionId::new("cc-preview"));
        assert!(preview.is_some(), "should find preview from matching adapter");
        assert_eq!(preview.unwrap().message_count, 5);
    }

    #[test]
    fn session_preview_unregistered_agent_returns_none() {
        let mut registry = AdapterRegistry::new();
        let adapter = MockAdapter::new("Claude Code", AgentKind::ClaudeCode);
        registry.register(Box::new(adapter));

        let preview = registry.session_preview(
            &AgentKind::Codex, // No Codex adapter registered
            &SessionId::new("some-id"),
        );
        assert!(preview.is_none(), "should return None for unregistered agent");
    }

    #[test]
    fn all_watch_paths_combines_and_deduplicates() {
        let mut registry = AdapterRegistry::new();

        let adapter1 =
            MockAdapter::new("Claude Code", AgentKind::ClaudeCode).with_watch_paths(vec![
                PathBuf::from("/home/user/.claude"),
                PathBuf::from("/shared/path"),
            ]);
        let adapter2 = MockAdapter::new("Codex", AgentKind::Codex).with_watch_paths(vec![
            PathBuf::from("/home/user/.codex"),
            PathBuf::from("/shared/path"), // duplicate
        ]);

        registry.register(Box::new(adapter1));
        registry.register(Box::new(adapter2));

        let paths = registry.all_watch_paths();
        // Should contain all unique paths
        assert!(paths.contains(&PathBuf::from("/home/user/.claude")), "should contain claude path");
        assert!(paths.contains(&PathBuf::from("/home/user/.codex")), "should contain codex path");
        assert!(paths.contains(&PathBuf::from("/shared/path")), "should contain shared path");

        // No duplicates
        let unique_count = {
            let mut sorted = paths.clone();
            sorted.sort();
            sorted.dedup();
            sorted.len()
        };
        assert_eq!(paths.len(), unique_count, "should have no duplicate paths");
    }

    #[test]
    fn adapter_names_in_registration_order() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter::new("Claude Code", AgentKind::ClaudeCode)));
        registry.register(Box::new(MockAdapter::new("Codex", AgentKind::Codex)));
        registry.register(Box::new(MockAdapter::new("Aider", AgentKind::Aider)));

        let names = registry.adapter_names();
        assert_eq!(names, vec!["Claude Code", "Codex", "Aider"]);
    }
}

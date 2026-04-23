//! Agent process detector.
//!
//! Detects known AI agent processes in a process tree rooted at a given PID.
//! This is the core detection logic; process tree traversal is platform-specific.

use veil_core::session::AgentKind;

/// A detected agent process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedAgent {
    /// Which agent was detected.
    pub kind: AgentKind,
    /// The process ID of the agent.
    pub pid: u32,
    /// The executable name that matched.
    pub exe_name: String,
}

/// A process entry from the process tree.
///
/// This is the platform-neutral representation. Platform-specific code
/// populates these by reading /proc (Linux), sysctl (macOS), etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// Process ID.
    pub pid: u32,
    /// Parent process ID.
    pub ppid: u32,
    /// Executable name (basename, not full path).
    pub name: String,
}

/// Known agent process names mapped to their `AgentKind`.
///
/// Each entry is a (`executable_name_pattern`, `AgentKind`) pair. The pattern
/// is matched against the process name (case-sensitive exact match on the
/// basename).
#[allow(dead_code)]
const KNOWN_AGENTS: &[(&str, AgentKind)] = &[
    ("claude", AgentKind::ClaudeCode),
    ("codex", AgentKind::Codex),
    ("opencode", AgentKind::OpenCode),
    ("aider", AgentKind::Aider),
];

/// Detect agent processes in a list of process entries that are descendants
/// of the given root PID.
///
/// Walks the process tree from `root_pid` downward, checking each process
/// name against `KNOWN_AGENTS`. Returns all matches (there could be multiple
/// agents, or an agent spawning sub-agents).
///
/// The `processes` slice should contain the full system process list (or at
/// least all descendants of the root). The function filters to only
/// descendants of `root_pid`.
pub fn detect_agents(_root_pid: u32, _processes: &[ProcessEntry]) -> Vec<DetectedAgent> {
    todo!()
}

/// Check if a single process name matches a known agent.
///
/// Returns the `AgentKind` if the name matches any known agent pattern.
pub fn identify_agent(_process_name: &str) -> Option<AgentKind> {
    todo!()
}

/// Build the set of all PIDs that are descendants of `root_pid`.
///
/// Performs a BFS/DFS from `root_pid` through parent-child relationships.
#[allow(dead_code)]
fn descendant_pids(_root_pid: u32, _processes: &[ProcessEntry]) -> Vec<u32> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper to build process entries ─────────────────────────────

    #[allow(clippy::similar_names)]
    fn proc(pid: u32, ppid: u32, name: &str) -> ProcessEntry {
        ProcessEntry { pid, ppid, name: name.to_string() }
    }

    // ── identify_agent unit tests ───────────────────────────────────

    #[test]
    fn identify_claude() {
        assert_eq!(identify_agent("claude"), Some(AgentKind::ClaudeCode));
    }

    #[test]
    fn identify_codex() {
        assert_eq!(identify_agent("codex"), Some(AgentKind::Codex));
    }

    #[test]
    fn identify_opencode() {
        assert_eq!(identify_agent("opencode"), Some(AgentKind::OpenCode));
    }

    #[test]
    fn identify_aider() {
        assert_eq!(identify_agent("aider"), Some(AgentKind::Aider));
    }

    #[test]
    fn identify_unknown_returns_none() {
        assert_eq!(identify_agent("vim"), None);
        assert_eq!(identify_agent("bash"), None);
        assert_eq!(identify_agent("node"), None);
    }

    #[test]
    fn identify_case_sensitive() {
        // "Claude" with capital C should NOT match "claude"
        assert_eq!(identify_agent("Claude"), None);
        assert_eq!(identify_agent("CLAUDE"), None);
        assert_eq!(identify_agent("Codex"), None);
    }

    // ── detect_agents: happy path ───────────────────────────────────

    #[test]
    fn detect_claude_as_direct_child() {
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, AgentKind::ClaudeCode);
        assert_eq!(result[0].pid, 200);
        assert_eq!(result[0].exe_name, "claude");
    }

    #[test]
    fn detect_codex_as_direct_child() {
        let procs = vec![proc(100, 1, "bash"), proc(201, 100, "codex")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, AgentKind::Codex);
    }

    #[test]
    fn detect_opencode_as_direct_child() {
        let procs = vec![proc(100, 1, "zsh"), proc(202, 100, "opencode")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, AgentKind::OpenCode);
    }

    #[test]
    fn detect_aider_as_direct_child() {
        let procs = vec![proc(100, 1, "fish"), proc(203, 100, "aider")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, AgentKind::Aider);
    }

    // ── detect_agents: process tree traversal ───────────────────────

    #[test]
    fn detect_agent_as_grandchild() {
        // root(100) -> shell(200) -> claude(300)
        let procs = vec![proc(100, 1, "init"), proc(200, 100, "zsh"), proc(300, 200, "claude")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 300);
    }

    #[test]
    fn detect_agent_as_deep_descendant() {
        // root(100) -> shell(200) -> tmux(300) -> shell(400) -> claude(500)
        let procs = vec![
            proc(100, 1, "init"),
            proc(200, 100, "zsh"),
            proc(300, 200, "tmux"),
            proc(400, 300, "bash"),
            proc(500, 400, "claude"),
        ];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 500);
    }

    #[test]
    fn does_not_detect_sibling_process() {
        // root(100) and sibling(101) are both children of init(1)
        // claude(200) is child of sibling(101), not descendant of root(100)
        let procs = vec![proc(100, 1, "zsh"), proc(101, 1, "zsh"), proc(200, 101, "claude")];
        let result = detect_agents(100, &procs);
        assert!(result.is_empty());
    }

    // ── detect_agents: edge cases ───────────────────────────────────

    #[test]
    fn empty_process_list() {
        let result = detect_agents(100, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn root_pid_not_in_list() {
        let procs = vec![proc(200, 100, "claude")];
        // Root PID 999 is not in the list, so 200 is not found as
        // a descendant. However, 200 has ppid=100, which is also not
        // 999, so nothing should be detected.
        let result = detect_agents(999, &procs);
        assert!(result.is_empty());
    }

    #[test]
    fn substring_agent_name_not_matched() {
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude-helper")];
        let result = detect_agents(100, &procs);
        assert!(result.is_empty());
    }

    #[test]
    fn multiple_agents_detected() {
        let procs = vec![proc(100, 1, "zsh"), proc(200, 100, "claude"), proc(300, 100, "aider")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 2);

        let kinds: Vec<AgentKind> = result.iter().map(|d| d.kind.clone()).collect();
        assert!(kinds.contains(&AgentKind::ClaudeCode));
        assert!(kinds.contains(&AgentKind::Aider));
    }

    #[test]
    fn root_pid_itself_is_agent() {
        let procs = vec![proc(100, 1, "claude")];
        let result = detect_agents(100, &procs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 100);
        assert_eq!(result[0].kind, AgentKind::ClaudeCode);
    }

    // ── KNOWN_AGENTS constant validation ────────────────────────────

    #[test]
    fn known_agents_has_all_expected_entries() {
        assert_eq!(KNOWN_AGENTS.len(), 4);
    }
}

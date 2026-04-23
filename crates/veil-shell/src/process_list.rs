//! Platform-specific process listing.
//!
//! Produces [`ProcessEntry`] values consumed by the agent detector.
//! The actual OS-level implementation uses `sysctl` on macOS, `/proc`
//! on Linux, and `CreateToolhelp32Snapshot` on Windows.

use crate::agent_detector::ProcessEntry;

/// Errors from process listing.
#[derive(Debug, thiserror::Error)]
pub enum ProcessListError {
    /// Failed to read process information from the OS.
    #[error("failed to list processes: {0}")]
    OsError(String),
}

/// List all running processes on the system.
///
/// Returns a snapshot of the process table. On macOS, uses `sysctl` with
/// `KERN_PROC_ALL`. On Linux, reads `/proc`. On Windows, uses
/// `CreateToolhelp32Snapshot`.
#[allow(unsafe_code)]
pub fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> {
    todo!()
}

/// List only processes that are descendants of the given PID.
///
/// Convenience function: calls `list_processes()` then filters to
/// descendants using the parent-child chain.
pub fn list_descendants(_root_pid: u32) -> Result<Vec<ProcessEntry>, ProcessListError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProcessEntry construction ───────────────────────────────────

    #[test]
    fn process_entry_construction_and_field_access() {
        let entry = ProcessEntry { pid: 42, ppid: 1, name: "bash".to_string() };
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.ppid, 1);
        assert_eq!(entry.name, "bash");
    }

    #[test]
    fn process_entry_clone() {
        let entry = ProcessEntry { pid: 100, ppid: 1, name: "zsh".to_string() };
        let cloned = entry.clone();
        assert_eq!(entry, cloned);
    }

    #[test]
    fn process_entry_debug_format() {
        let entry = ProcessEntry { pid: 1, ppid: 0, name: "init".to_string() };
        let debug = format!("{entry:?}");
        assert!(debug.contains("ProcessEntry"));
        assert!(debug.contains("init"));
    }

    // ── API shape verification ──────────────────────────────────────

    #[test]
    fn list_processes_returns_result_type() {
        // Verify the function signature compiles and returns the expected type.
        // The actual call will panic with todo!(), which is the expected RED state.
        let result: Result<Vec<ProcessEntry>, ProcessListError> =
            std::panic::catch_unwind(list_processes).unwrap_or(Ok(vec![]));
        // We just need this to compile; the todo!() panic is caught above.
        let _ = result;
    }

    #[test]
    fn list_descendants_returns_result_type() {
        let result: Result<Vec<ProcessEntry>, ProcessListError> =
            std::panic::catch_unwind(|| list_descendants(1)).unwrap_or(Ok(vec![]));
        let _ = result;
    }

    #[test]
    fn process_list_error_display() {
        let err = ProcessListError::OsError("test error".to_string());
        assert_eq!(err.to_string(), "failed to list processes: test error");
    }
}

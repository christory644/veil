//! Session discovery for Claude Code.
//!
//! Scans `~/.claude/projects/` to find all session JSONL files and determine
//! which project directory they belong to.

use std::path::{Path, PathBuf};

/// A discovered session file on disk, not yet parsed.
///
/// Fields beyond `jsonl_path` are populated for downstream consumers and
/// future features (VEI-27 metadata extraction).
#[derive(Debug)]
#[allow(dead_code)] // fields used in tests and reserved for VEI-27
pub struct DiscoveredSession {
    /// Path to the session JSONL file.
    pub jsonl_path: PathBuf,
    /// Session UUID extracted from the filename.
    pub session_id: String,
    /// Project directory (parent of the JSONL file).
    pub project_dir: PathBuf,
    /// Encoded project name (directory name, e.g., "-Users-user-repos-foo").
    pub project_hash: String,
}

/// Scan a base directory for Claude Code session JSONL files.
///
/// Looks for `<base>/<project-hash>/<uuid>.jsonl` files.
/// Skips subagent files (those are inside `<uuid>/subagents/`).
/// Returns one `DiscoveredSession` per JSONL file found.
pub fn discover_sessions(base_dir: &Path) -> Vec<DiscoveredSession> {
    let mut sessions = Vec::new();

    // If the base directory doesn't exist or isn't a directory, return empty.
    let Ok(project_dirs) = std::fs::read_dir(base_dir) else {
        return sessions;
    };

    // First level: project directories
    for project_entry in project_dirs.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let Some(project_hash) =
            project_path.file_name().and_then(|n| n.to_str()).map(ToString::to_string)
        else {
            continue;
        };

        // Second level: JSONL files within each project directory
        let Ok(files) = std::fs::read_dir(&project_path) else {
            continue;
        };

        for file_entry in files.flatten() {
            let file_path = file_entry.path();

            // Skip files inside subagent directories (/subagents/...)
            if file_path.components().any(|c| c.as_os_str() == "subagents") {
                continue;
            }

            // Only process files (not directories)
            if !file_path.is_file() {
                continue;
            }

            // Extract filename and check for UUID pattern
            let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if let Some(session_id) = extract_session_id(filename) {
                sessions.push(DiscoveredSession {
                    jsonl_path: file_path,
                    session_id,
                    project_dir: project_path.clone(),
                    project_hash: project_hash.clone(),
                });
            }
        }
    }

    sessions
}

/// Resolve the Claude Code projects directory.
/// Returns `~/.claude/projects/` with home directory expansion.
/// Returns `None` if the directory does not exist.
pub fn resolve_projects_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let projects_dir = home.join(".claude").join("projects");
    if projects_dir.is_dir() {
        Some(projects_dir)
    } else {
        None
    }
}

/// Extract a session UUID from a JSONL filename.
/// Returns `None` if the filename doesn't match the UUID pattern.
fn extract_session_id(filename: &str) -> Option<String> {
    // Must end with .jsonl
    let stem = filename.strip_suffix(".jsonl")?;

    // Validate UUID format: 8-4-4-4-12 hex characters
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() != 5 {
        return None;
    }

    let expected_lengths = [8, 4, 4, 4, 12];
    for (part, &expected_len) in parts.iter().zip(&expected_lengths) {
        if part.len() != expected_len || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
    }

    Some(stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a temp directory structure mimicking `~/.claude/projects/`.
    fn setup_temp_projects_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("should create temp dir");

        // Create a project directory with two session JSONL files
        let project_dir = tmp.path().join("-Users-testuser-repos-myproject");
        fs::create_dir_all(&project_dir).expect("should create project dir");

        // Valid session JSONL files
        fs::write(
            project_dir.join("11111111-1111-1111-1111-111111111111.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
        )
        .expect("should write session file");

        fs::write(
            project_dir.join("22222222-2222-2222-2222-222222222222.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":"world"}}"#,
        )
        .expect("should write session file");

        // Non-JSONL files that should be ignored
        fs::write(project_dir.join("notes.txt"), "not a session").expect("should write txt");
        fs::write(project_dir.join("data.json"), "{}").expect("should write json");

        // Subagent directory that should be ignored
        let subagent_dir =
            project_dir.join("11111111-1111-1111-1111-111111111111").join("subagents");
        fs::create_dir_all(&subagent_dir).expect("should create subagent dir");
        fs::write(
            subagent_dir.join("agent-abc123.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":"subagent msg"}}"#,
        )
        .expect("should write subagent file");

        tmp
    }

    #[test]
    fn discover_sessions_finds_valid_jsonl_files() {
        let tmp = setup_temp_projects_dir();
        let sessions = discover_sessions(tmp.path());
        assert_eq!(
            sessions.len(),
            2,
            "should find exactly 2 session files, found {}",
            sessions.len()
        );
    }

    #[test]
    fn discover_sessions_skips_non_jsonl_files() {
        let tmp = setup_temp_projects_dir();
        let sessions = discover_sessions(tmp.path());
        let paths: Vec<_> = sessions.iter().map(|s| s.jsonl_path.clone()).collect();
        for path in &paths {
            assert!(
                path.extension().is_some_and(|ext| ext == "jsonl"),
                "all discovered files should be .jsonl, got: {path:?}"
            );
        }
    }

    #[test]
    fn discover_sessions_skips_subagent_jsonl_files() {
        let tmp = setup_temp_projects_dir();
        let sessions = discover_sessions(tmp.path());
        for session in &sessions {
            assert!(
                !session.jsonl_path.to_string_lossy().contains("subagents"),
                "should not discover subagent files: {:?}",
                session.jsonl_path
            );
        }
    }

    #[test]
    fn discover_sessions_empty_directory() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let sessions = discover_sessions(tmp.path());
        assert!(sessions.is_empty(), "empty directory should produce no sessions");
    }

    #[test]
    fn discover_sessions_nonexistent_directory() {
        let sessions = discover_sessions(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(
            sessions.is_empty(),
            "nonexistent directory should produce no sessions, not an error"
        );
    }

    #[test]
    fn extract_session_id_valid_uuid_filename() {
        let result = extract_session_id("11111111-1111-1111-1111-111111111111.jsonl");
        assert_eq!(result.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
    }

    #[test]
    fn extract_session_id_non_uuid_filename() {
        let result = extract_session_id("notes.jsonl");
        assert!(result.is_none(), "non-UUID filename should return None, got {result:?}");
    }

    #[test]
    fn extract_session_id_non_jsonl_extension() {
        let result = extract_session_id("11111111-1111-1111-1111-111111111111.json");
        assert!(result.is_none(), "non-.jsonl extension should return None, got {result:?}");
    }

    #[test]
    fn extract_session_id_extra_characters_in_filename() {
        let result = extract_session_id("prefix-11111111-1111-1111-1111-111111111111.jsonl");
        assert!(
            result.is_none(),
            "filename with extra characters should return None, got {result:?}"
        );
    }

    #[test]
    fn discovered_sessions_include_correct_project_hash() {
        let tmp = setup_temp_projects_dir();
        let sessions = discover_sessions(tmp.path());
        for session in &sessions {
            assert_eq!(
                session.project_hash, "-Users-testuser-repos-myproject",
                "project_hash should match directory name"
            );
        }
    }

    #[test]
    fn multiple_sessions_in_one_project_discovered_separately() {
        let tmp = setup_temp_projects_dir();
        let sessions = discover_sessions(tmp.path());
        let ids: Vec<_> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(
            ids.contains(&"11111111-1111-1111-1111-111111111111"),
            "should discover first session"
        );
        assert!(
            ids.contains(&"22222222-2222-2222-2222-222222222222"),
            "should discover second session"
        );
    }
}

//! Environment detector.
//!
//! Detects environment characteristics of a working directory by checking
//! for marker files and directories.

use std::path::Path;

/// A detected environment characteristic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnvKind {
    /// Git repository (`.git` directory or file exists).
    Git,
    /// Node.js project (`package.json` exists).
    Node,
    /// Python project (`pyproject.toml`, `setup.py`, or `requirements.txt` exists).
    Python,
    /// Rust project (`Cargo.toml` exists).
    Rust,
    /// Go project (`go.mod` exists).
    Go,
    /// Java project (`pom.xml` or `build.gradle` exists).
    Java,
    /// Nix project (`flake.nix` or `shell.nix` exists).
    Nix,
}

/// Result of environment detection for a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvReport {
    /// The set of detected environment characteristics.
    /// A directory can have multiple (e.g., Git + Rust + Nix).
    pub kinds: Vec<EnvKind>,
}

/// Detect environment characteristics of a directory.
///
/// Checks for marker files in the given directory (not recursive -- only
/// checks the directory itself, not parents). Returns an `EnvReport`
/// with all detected characteristics.
///
/// If the directory does not exist or is not readable, returns an empty report.
pub fn detect_env(_dir: &Path) -> EnvReport {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── Helper ──────────────────────────────────────────────────────

    fn create_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "").unwrap();
    }

    fn create_dir(dir: &Path, name: &str) {
        std::fs::create_dir(dir.join(name)).unwrap();
    }

    // ── Happy path: each marker ─────────────────────────────────────

    #[test]
    fn detect_git_directory() {
        let tmp = TempDir::new().unwrap();
        create_dir(tmp.path(), ".git");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Git));
    }

    #[test]
    fn detect_rust_project() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "Cargo.toml");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Rust));
    }

    #[test]
    fn detect_node_project() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "package.json");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Node));
    }

    #[test]
    fn detect_go_project() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "go.mod");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Go));
    }

    #[test]
    fn detect_python_via_pyproject() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "pyproject.toml");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Python));
    }

    #[test]
    fn detect_python_via_setup_py() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "setup.py");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Python));
    }

    #[test]
    fn detect_python_via_requirements_txt() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "requirements.txt");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Python));
    }

    #[test]
    fn detect_java_via_pom() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "pom.xml");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Java));
    }

    #[test]
    fn detect_java_via_build_gradle() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "build.gradle");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Java));
    }

    #[test]
    fn detect_java_via_build_gradle_kts() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "build.gradle.kts");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Java));
    }

    #[test]
    fn detect_nix_via_flake() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "flake.nix");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Nix));
    }

    #[test]
    fn detect_nix_via_shell_nix() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "shell.nix");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Nix));
    }

    #[test]
    fn detect_nix_via_default_nix() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "default.nix");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Nix));
    }

    // ── Multiple markers ────────────────────────────────────────────

    #[test]
    fn detect_multiple_environments() {
        let tmp = TempDir::new().unwrap();
        create_dir(tmp.path(), ".git");
        create_file(tmp.path(), "Cargo.toml");
        create_file(tmp.path(), "flake.nix");
        let report = detect_env(tmp.path());
        assert_eq!(report.kinds.len(), 3);
        assert!(report.kinds.contains(&EnvKind::Git));
        assert!(report.kinds.contains(&EnvKind::Rust));
        assert!(report.kinds.contains(&EnvKind::Nix));
    }

    // ── No markers ──────────────────────────────────────────────────

    #[test]
    fn empty_directory_has_no_environments() {
        let tmp = TempDir::new().unwrap();
        let report = detect_env(tmp.path());
        assert!(report.kinds.is_empty());
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn nonexistent_path_returns_empty() {
        let report = detect_env(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(report.kinds.is_empty());
    }

    #[test]
    fn path_to_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("some_file");
        std::fs::write(&file_path, "").unwrap();
        let report = detect_env(&file_path);
        assert!(report.kinds.is_empty());
    }

    #[test]
    fn git_as_file_detected() {
        // Git worktrees use a `.git` file instead of a directory.
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".git");
        let report = detect_env(tmp.path());
        assert!(report.kinds.contains(&EnvKind::Git));
    }

    #[test]
    fn markers_in_parent_not_detected() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "Cargo.toml");
        let child = tmp.path().join("src");
        std::fs::create_dir(&child).unwrap();
        let report = detect_env(&child);
        // src/ does not have Cargo.toml, parent does -- should NOT detect
        assert!(!report.kinds.contains(&EnvKind::Rust));
    }

    // ── Deterministic ordering ──────────────────────────────────────

    #[test]
    fn kinds_are_sorted() {
        let tmp = TempDir::new().unwrap();
        // Create markers for multiple envs
        create_dir(tmp.path(), ".git");
        create_file(tmp.path(), "package.json");
        create_file(tmp.path(), "Cargo.toml");
        create_file(tmp.path(), "flake.nix");
        let report = detect_env(tmp.path());
        let sorted: Vec<EnvKind> = {
            let mut v = report.kinds.clone();
            v.sort();
            v
        };
        assert_eq!(report.kinds, sorted, "kinds should be sorted by discriminant order");
    }
}

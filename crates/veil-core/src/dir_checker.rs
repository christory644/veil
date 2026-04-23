//! Directory existence checker.
//!
//! Trivial module that checks whether a working directory still exists
//! on disk. Separated for consistency and testability.

use std::path::Path;

use crate::live_state::DirState;

/// Checks whether a working directory still exists on disk.
pub struct DirChecker;

impl DirChecker {
    /// Check if a directory exists.
    pub fn check(path: &Path) -> DirState {
        if path.exists() {
            DirState::Exists
        } else {
            DirState::Missing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ================================================================
    // Happy path
    // ================================================================

    #[test]
    fn existing_directory_returns_exists() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        let state = DirChecker::check(tmp.path());
        assert_eq!(state, DirState::Exists);
    }

    #[test]
    fn nonexistent_path_returns_missing() {
        let state = DirChecker::check(Path::new("/nonexistent/directory/that/cannot/exist"));
        assert_eq!(state, DirState::Missing);
    }

    // ================================================================
    // Edge cases
    // ================================================================

    #[test]
    fn empty_string_path_returns_missing() {
        let state = DirChecker::check(Path::new(""));
        assert_eq!(state, DirState::Missing);
    }

    #[test]
    fn file_path_returns_exists() {
        // Path::exists() returns true for files too — document this behavior.
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        let file_path = tmp.path().join("test_file.txt");
        std::fs::write(&file_path, "content").expect("write should succeed");

        let state = DirChecker::check(&file_path);
        // Path::exists() returns true for files, so DirChecker returns Exists
        // even for a file. This is by design (documented behavior).
        assert_eq!(state, DirState::Exists);
    }

    #[test]
    fn deleted_tempdir_returns_missing() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        let path = tmp.path().to_path_buf();
        // Verify it exists first.
        assert_eq!(DirChecker::check(&path), DirState::Exists);
        // Drop the tempdir to delete it.
        drop(tmp);
        // Now it should be missing.
        assert_eq!(DirChecker::check(&path), DirState::Missing);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_existing_dir_returns_exists() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        let target = tmp.path().join("real_dir");
        std::fs::create_dir(&target).expect("create_dir should succeed");
        let link = tmp.path().join("link_dir");
        std::os::unix::fs::symlink(&target, &link).expect("symlink should succeed");

        let state = DirChecker::check(&link);
        assert_eq!(state, DirState::Exists);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_deleted_dir_returns_missing() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        let target = tmp.path().join("will_delete");
        std::fs::create_dir(&target).expect("create_dir should succeed");
        let link = tmp.path().join("dangling_link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink should succeed");

        // Delete the target — symlink becomes dangling.
        std::fs::remove_dir(&target).expect("remove_dir should succeed");

        let state = DirChecker::check(&link);
        // Path::exists() returns false for dangling symlinks.
        assert_eq!(state, DirState::Missing);
    }
}

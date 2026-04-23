//! Git branch existence checker.
//!
//! Shells out to `git` via `std::process::Command` to check whether branches
//! exist in repositories. Supports batching multiple branch checks per repo.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::live_state::BranchState;

/// Checks git branch existence by shelling out to `git`.
pub struct GitChecker;

/// A request to check one branch in one repo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchCheckRequest {
    /// Path to the git repository (working directory).
    pub repo_path: PathBuf,
    /// Branch name to check.
    pub branch_name: String,
}

/// Result of checking one branch.
#[derive(Debug, Clone)]
pub struct BranchCheckResult {
    /// The original request.
    pub request: BranchCheckRequest,
    /// The determined state.
    pub state: BranchState,
}

impl GitChecker {
    /// Check a single branch in a single repo.
    ///
    /// Runs `git -C <repo_path> branch --list <branch_name>` and checks
    /// whether the output is non-empty.
    pub fn check_branch(repo_path: &Path, branch_name: &str) -> BranchState {
        let output = Command::new("git")
            .args(["-C"])
            .arg(repo_path)
            .args(["branch", "--list", branch_name])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                if stdout.trim().is_empty() {
                    BranchState::Deleted
                } else {
                    BranchState::Exists
                }
            }
            _ => BranchState::Unknown,
        }
    }

    /// Check multiple branches, batching by repo.
    ///
    /// For each unique `repo_path`, runs a single `git -C <repo_path> branch --list`
    /// (with all branch names for that repo) and parses the output.
    /// Returns results in the same order as the input requests.
    pub fn check_branches(requests: &[BranchCheckRequest]) -> Vec<BranchCheckResult> {
        if requests.is_empty() {
            return Vec::new();
        }

        // Group request indices by repo_path.
        let mut repo_groups: HashMap<&Path, Vec<usize>> = HashMap::new();
        for (idx, req) in requests.iter().enumerate() {
            repo_groups.entry(req.repo_path.as_path()).or_default().push(idx);
        }

        // Pre-allocate results with Unknown as default.
        let mut results: Vec<Option<BranchState>> = vec![None; requests.len()];

        for (repo_path, indices) in &repo_groups {
            // Collect all branch names for this repo.
            let branch_names: Vec<&str> =
                indices.iter().map(|&i| requests[i].branch_name.as_str()).collect();

            let mut cmd = Command::new("git");
            cmd.args(["-C"]).arg(repo_path).args(["branch", "--list"]);
            for name in &branch_names {
                cmd.arg(name);
            }

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    // Parse output lines: each existing branch appears as a line,
                    // possibly prefixed with `* ` for the current branch.
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let found_branches: HashSet<&str> = stdout
                        .lines()
                        .map(|line| {
                            let trimmed = line.trim();
                            trimmed.strip_prefix("* ").unwrap_or(trimmed)
                        })
                        .filter(|s| !s.is_empty())
                        .collect();

                    for &idx in indices {
                        let name = requests[idx].branch_name.as_str();
                        if found_branches.contains(name) {
                            results[idx] = Some(BranchState::Exists);
                        } else {
                            results[idx] = Some(BranchState::Deleted);
                        }
                    }
                }
                _ => {
                    // Git command failed for this repo -- mark all as Unknown.
                    for &idx in indices {
                        results[idx] = Some(BranchState::Unknown);
                    }
                }
            }
        }

        requests
            .iter()
            .zip(results)
            .map(|(req, state)| BranchCheckResult {
                request: req.clone(),
                state: state.unwrap_or(BranchState::Unknown),
            })
            .collect()
    }

    /// Check whether `git` is available on the system PATH.
    pub fn is_available() -> bool {
        Command::new("git").arg("--version").output().is_ok_and(|output| output.status.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temporary git repo with an initial commit.
    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("git init should succeed");

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .expect("git config email should succeed");

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()
            .expect("git config name should succeed");

        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "initial commit"])
            .current_dir(dir)
            .output()
            .expect("git commit should succeed");
    }

    /// Helper: create a branch in a git repo.
    fn create_branch(dir: &Path, branch_name: &str) {
        Command::new("git")
            .args(["branch", branch_name])
            .current_dir(dir)
            .output()
            .expect("git branch should succeed");
    }

    /// Helper: delete a branch in a git repo.
    fn delete_branch(dir: &Path, branch_name: &str) {
        Command::new("git")
            .args(["branch", "-d", branch_name])
            .current_dir(dir)
            .output()
            .expect("git branch -d should succeed");
    }

    // ================================================================
    // is_available
    // ================================================================

    #[test]
    fn is_available_returns_true_when_git_exists() {
        // In any CI or dev environment, git should be available.
        assert!(GitChecker::is_available(), "git should be available in test environment");
    }

    // ================================================================
    // check_branch — happy path
    // ================================================================

    #[test]
    fn check_branch_existing_branch_returns_exists() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());
        create_branch(tmp.path(), "feature/test-branch");

        let state = GitChecker::check_branch(tmp.path(), "feature/test-branch");
        assert_eq!(state, BranchState::Exists);
    }

    #[test]
    fn check_branch_deleted_branch_returns_deleted() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());
        create_branch(tmp.path(), "feature/to-delete");
        delete_branch(tmp.path(), "feature/to-delete");

        let state = GitChecker::check_branch(tmp.path(), "feature/to-delete");
        assert_eq!(state, BranchState::Deleted);
    }

    #[test]
    fn check_branch_nonexistent_branch_returns_deleted() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());

        let state = GitChecker::check_branch(tmp.path(), "does-not-exist");
        assert_eq!(state, BranchState::Deleted);
    }

    #[test]
    fn check_branch_default_branch_returns_exists() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());

        // The default branch (main or master) should exist.
        // Detect which default branch was created.
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(tmp.path())
            .output()
            .expect("git branch --show-current should succeed");
        let default_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let state = GitChecker::check_branch(tmp.path(), &default_branch);
        assert_eq!(state, BranchState::Exists);
    }

    // ================================================================
    // check_branch — error cases
    // ================================================================

    #[test]
    fn check_branch_nonexistent_directory_returns_unknown() {
        let state =
            GitChecker::check_branch(Path::new("/nonexistent/dir/that/cannot/exist"), "main");
        assert_eq!(state, BranchState::Unknown);
    }

    #[test]
    fn check_branch_non_git_directory_returns_unknown() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        // tmp is not a git repo
        let state = GitChecker::check_branch(tmp.path(), "main");
        assert_eq!(state, BranchState::Unknown);
    }

    #[test]
    fn check_branch_empty_branch_name_returns_unknown() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());

        let state = GitChecker::check_branch(tmp.path(), "");
        // Empty branch name is invalid; should not panic, should return Unknown or Deleted.
        assert!(
            state == BranchState::Unknown || state == BranchState::Deleted,
            "empty branch name should return Unknown or Deleted, got {state:?}"
        );
    }

    // ================================================================
    // check_branches — batch
    // ================================================================

    #[test]
    fn check_branches_multiple_in_same_repo() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());
        create_branch(tmp.path(), "feat/one");
        create_branch(tmp.path(), "feat/two");

        let requests = vec![
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "feat/one".to_string(),
            },
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "feat/two".to_string(),
            },
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "feat/missing".to_string(),
            },
        ];

        let results = GitChecker::check_branches(&requests);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].state, BranchState::Exists);
        assert_eq!(results[1].state, BranchState::Exists);
        assert_eq!(results[2].state, BranchState::Deleted);
    }

    #[test]
    fn check_branches_cross_repo_batch() {
        let tmp1 = tempfile::tempdir().expect("tempdir should succeed");
        let tmp2 = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp1.path());
        init_git_repo(tmp2.path());
        create_branch(tmp1.path(), "feat/repo1");
        create_branch(tmp2.path(), "feat/repo2");

        let requests = vec![
            BranchCheckRequest {
                repo_path: tmp1.path().to_path_buf(),
                branch_name: "feat/repo1".to_string(),
            },
            BranchCheckRequest {
                repo_path: tmp2.path().to_path_buf(),
                branch_name: "feat/repo2".to_string(),
            },
        ];

        let results = GitChecker::check_branches(&requests);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].state, BranchState::Exists);
        assert_eq!(results[1].state, BranchState::Exists);
    }

    #[test]
    fn check_branches_preserves_request_order() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());
        create_branch(tmp.path(), "alpha");
        create_branch(tmp.path(), "beta");

        let requests = vec![
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "beta".to_string(),
            },
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "alpha".to_string(),
            },
        ];

        let results = GitChecker::check_branches(&requests);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].request.branch_name, "beta");
        assert_eq!(results[1].request.branch_name, "alpha");
    }

    #[test]
    fn check_branches_empty_input_returns_empty() {
        let results = GitChecker::check_branches(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn check_branches_bad_repo_returns_unknown_for_those() {
        let tmp = tempfile::tempdir().expect("tempdir should succeed");
        init_git_repo(tmp.path());
        create_branch(tmp.path(), "feat/good");

        let requests = vec![
            BranchCheckRequest {
                repo_path: tmp.path().to_path_buf(),
                branch_name: "feat/good".to_string(),
            },
            BranchCheckRequest {
                repo_path: PathBuf::from("/nonexistent/repo"),
                branch_name: "main".to_string(),
            },
        ];

        let results = GitChecker::check_branches(&requests);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].state, BranchState::Exists);
        assert_eq!(results[1].state, BranchState::Unknown);
    }
}

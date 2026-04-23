//! Live state resolver — coordinates cache lookups and checker invocations.
//!
//! Given a list of session metadata, resolves the live state of branches,
//! PRs, and directories. Consults the cache first, checks only stale/missing
//! entries, and writes fresh results back.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{Duration, Utc};
use veil_core::dir_checker::DirChecker;
use veil_core::git_checker::GitChecker;
use veil_core::live_state::{BranchState, DirState, LiveStatus, PrState};
use veil_core::pr_checker::PrChecker;
use veil_core::session::SessionId;

use crate::live_state_cache::{CheckType, LiveStateCache};
use crate::store::StoreError;

/// Configuration for the resolver.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Maximum age of a cached branch check before re-checking.
    pub branch_ttl: Duration,
    /// Maximum age of a cached PR check before re-checking.
    pub pr_ttl: Duration,
    /// Maximum age of a cached directory check before re-checking.
    pub dir_ttl: Duration,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            branch_ttl: Duration::seconds(60),
            pr_ttl: Duration::seconds(120),
            dir_ttl: Duration::seconds(300),
        }
    }
}

/// Input to the resolver: one session's metadata that might need live checks.
#[derive(Debug, Clone)]
pub struct SessionCheckInput {
    /// Session identifier.
    pub session_id: SessionId,
    /// Path to the git repository, if known.
    pub repo_path: Option<PathBuf>,
    /// Branch name, if known.
    pub branch_name: Option<String>,
    /// PR number, if known.
    pub pr_number: Option<u64>,
    /// Working directory path.
    pub working_dir: PathBuf,
}

/// Resolves live state for sessions by checking cache, then shelling out
/// for stale/missing entries.
pub struct LiveStateResolver<'a> {
    cache: LiveStateCache<'a>,
    config: ResolverConfig,
}

impl<'a> LiveStateResolver<'a> {
    /// Create a new resolver with the given cache and configuration.
    pub fn new(cache: LiveStateCache<'a>, config: ResolverConfig) -> Self {
        Self { cache, config }
    }

    /// Resolve live state for a batch of sessions.
    ///
    /// Returns a map from `SessionId` to `LiveStatus`. Sessions that have no
    /// branch/PR/dir metadata will have an empty (default) `LiveStatus`.
    pub fn resolve(
        &self,
        inputs: &[SessionCheckInput],
    ) -> Result<HashMap<SessionId, LiveStatus>, StoreError> {
        let mut results = HashMap::with_capacity(inputs.len());

        for input in inputs {
            let branch = self.resolve_branch(input)?;
            let pr = self.resolve_pr(input)?;
            let dir = self.resolve_dir(input)?;

            results.insert(input.session_id.clone(), LiveStatus { branch, pr, dir });
        }

        Ok(results)
    }

    /// Resolve branch state for a single session input.
    ///
    /// Returns `None` if the session has no branch metadata (no `branch_name`
    /// or no `repo_path`). Returns `Some(BranchState)` otherwise.
    fn resolve_branch(&self, input: &SessionCheckInput) -> Result<Option<BranchState>, StoreError> {
        let (Some(repo_path), Some(branch_name)) =
            (input.repo_path.as_ref(), input.branch_name.as_ref())
        else {
            return Ok(None);
        };

        let cache_key = format!("{}::{}", repo_path.display(), branch_name);

        // Check cache first.
        if let Some(cached) =
            self.cache.get(CheckType::Branch, &cache_key, self.config.branch_ttl)?
        {
            // FromStr on BranchState maps unrecognised values to Unknown.
            return Ok(Some(cached.state.parse().unwrap_or(BranchState::Unknown)));
        }

        // Cache miss -- check via git.
        let state = GitChecker::check_branch(repo_path, branch_name);
        self.cache.put(CheckType::Branch, &cache_key, &state.to_string(), Utc::now())?;

        Ok(Some(state))
    }

    /// Resolve PR state for a single session input.
    ///
    /// Returns `None` if the session has no PR metadata (no `pr_number`
    /// or no `repo_path`). Returns `Some(PrState)` otherwise.
    fn resolve_pr(&self, input: &SessionCheckInput) -> Result<Option<PrState>, StoreError> {
        let (Some(repo_path), Some(pr_number)) = (input.repo_path.as_ref(), input.pr_number) else {
            return Ok(None);
        };

        let cache_key = format!("{}::{}", repo_path.display(), pr_number);

        // Check cache first.
        if let Some(cached) = self.cache.get(CheckType::Pr, &cache_key, self.config.pr_ttl)? {
            return Ok(Some(cached.state.parse().unwrap_or(PrState::Unknown)));
        }

        // Cache miss -- check via gh.
        let state = PrChecker::check_pr(repo_path, pr_number);
        self.cache.put(CheckType::Pr, &cache_key, &state.to_string(), Utc::now())?;

        Ok(Some(state))
    }

    /// Resolve directory state for a single session input.
    ///
    /// Always returns `Some(DirState)` because every session has a working dir.
    fn resolve_dir(&self, input: &SessionCheckInput) -> Result<Option<DirState>, StoreError> {
        let cache_key = input.working_dir.display().to_string();

        // Check cache first.
        if let Some(cached) = self.cache.get(CheckType::Dir, &cache_key, self.config.dir_ttl)? {
            return Ok(Some(cached.state.parse().unwrap_or(DirState::Missing)));
        }

        // Cache miss -- check filesystem.
        let state = DirChecker::check(&input.working_dir);
        self.cache.put(CheckType::Dir, &cache_key, &state.to_string(), Utc::now())?;

        Ok(Some(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: set up an in-memory `SQLite` connection and run cache migrations.
    fn setup() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory should succeed");
        let cache = LiveStateCache::new(&conn);
        cache.run_migrations().expect("migrations should succeed");
        conn
    }

    fn make_input(
        id: &str,
        repo: Option<&str>,
        branch: Option<&str>,
        pr: Option<u64>,
        working_dir: &str,
    ) -> SessionCheckInput {
        SessionCheckInput {
            session_id: SessionId::new(id),
            repo_path: repo.map(PathBuf::from),
            branch_name: branch.map(String::from),
            pr_number: pr,
            working_dir: PathBuf::from(working_dir),
        }
    }

    // ================================================================
    // Constructor
    // ================================================================

    #[test]
    fn new_creates_resolver() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let config = ResolverConfig::default();
        let _resolver = LiveStateResolver::new(cache, config);
    }

    // ================================================================
    // Empty input
    // ================================================================

    #[test]
    fn resolve_empty_input_returns_empty_map() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let result = resolver.resolve(&[]).expect("resolve should succeed");
        assert!(result.is_empty(), "empty input should return empty map");
    }

    // ================================================================
    // Happy path — sessions with metadata
    // ================================================================

    #[test]
    fn resolve_session_with_all_metadata() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let inputs = vec![make_input("s1", Some("/tmp/repo"), Some("main"), Some(42), "/tmp/repo")];

        let result = resolver.resolve(&inputs).expect("resolve should succeed");
        assert!(result.contains_key(&SessionId::new("s1")));
        let status = &result[&SessionId::new("s1")];
        // Branch, PR, and dir should all have a state (not None).
        assert!(status.branch.is_some(), "branch should be resolved");
        assert!(status.pr.is_some(), "pr should be resolved");
        assert!(status.dir.is_some(), "dir should be resolved");
    }

    #[test]
    fn resolve_session_with_no_branch_or_pr() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let inputs = vec![make_input("s1", None, None, None, "/tmp/project")];

        let result = resolver.resolve(&inputs).expect("resolve should succeed");
        let status = &result[&SessionId::new("s1")];
        assert!(status.branch.is_none(), "no branch metadata means None, not Unknown");
        assert!(status.pr.is_none(), "no PR metadata means None, not Unknown");
        assert!(status.dir.is_some(), "dir should always be checked");
    }

    #[test]
    fn resolve_multiple_sessions() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let inputs = vec![
            make_input("s1", Some("/tmp/repo"), Some("feat/a"), None, "/tmp/repo"),
            make_input("s2", None, None, Some(99), "/tmp/other"),
            make_input("s3", Some("/tmp/repo"), Some("feat/b"), Some(100), "/tmp/repo"),
        ];

        let result = resolver.resolve(&inputs).expect("resolve should succeed");
        assert_eq!(result.len(), 3, "should have a result for each input session");
        assert!(result.contains_key(&SessionId::new("s1")));
        assert!(result.contains_key(&SessionId::new("s2")));
        assert!(result.contains_key(&SessionId::new("s3")));
    }

    // ================================================================
    // Cache behavior
    // ================================================================

    #[test]
    fn resolve_uses_cache_on_second_call() {
        let conn = setup();

        // First resolve: populates cache.
        {
            let cache = LiveStateCache::new(&conn);
            let resolver = LiveStateResolver::new(cache, ResolverConfig::default());
            let inputs = vec![make_input("s1", None, None, None, "/tmp/project")];
            resolver.resolve(&inputs).expect("first resolve should succeed");
        }

        // Second resolve: should use cache (we verify it doesn't error).
        {
            let cache = LiveStateCache::new(&conn);
            let resolver = LiveStateResolver::new(cache, ResolverConfig::default());
            let inputs = vec![make_input("s1", None, None, None, "/tmp/project")];
            let result = resolver.resolve(&inputs).expect("second resolve should succeed");
            assert!(result.contains_key(&SessionId::new("s1")));
        }
    }

    // ================================================================
    // Default config
    // ================================================================

    #[test]
    fn resolver_config_default_values() {
        let config = ResolverConfig::default();
        assert_eq!(config.branch_ttl, Duration::seconds(60));
        assert_eq!(config.pr_ttl, Duration::seconds(120));
        assert_eq!(config.dir_ttl, Duration::seconds(300));
    }

    // ================================================================
    // Graceful degradation
    // ================================================================

    #[test]
    fn resolve_nonexistent_working_dir_returns_missing() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let inputs =
            vec![make_input("s1", None, None, None, "/nonexistent/directory/that/cannot/exist")];

        let result = resolver.resolve(&inputs).expect("resolve should succeed");
        let status = &result[&SessionId::new("s1")];
        assert_eq!(status.dir, Some(DirState::Missing), "nonexistent dir should be Missing");
    }

    #[test]
    fn resolve_nonexistent_repo_branch_returns_unknown() {
        let conn = setup();
        let cache = LiveStateCache::new(&conn);
        let resolver = LiveStateResolver::new(cache, ResolverConfig::default());

        let inputs = vec![make_input("s1", Some("/nonexistent/repo"), Some("main"), None, "/tmp")];

        let result = resolver.resolve(&inputs).expect("resolve should succeed");
        let status = &result[&SessionId::new("s1")];
        assert_eq!(
            status.branch,
            Some(BranchState::Unknown),
            "branch check on nonexistent repo should return Unknown"
        );
    }

    // ================================================================
    // SessionCheckInput
    // ================================================================

    #[test]
    fn session_check_input_clone() {
        let input = make_input("s1", Some("/repo"), Some("main"), Some(42), "/repo");
        let cloned = input.clone();
        assert_eq!(cloned.session_id, SessionId::new("s1"));
        assert_eq!(cloned.branch_name, Some("main".to_string()));
        assert_eq!(cloned.pr_number, Some(42));
    }

    #[test]
    fn session_check_input_all_none_optionals() {
        let input = make_input("s1", None, None, None, "/tmp");
        assert!(input.repo_path.is_none());
        assert!(input.branch_name.is_none());
        assert!(input.pr_number.is_none());
    }
}

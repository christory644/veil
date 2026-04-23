//! Update notification system — version checking, install method detection,
//! and orchestrated update flow.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::UpdatesConfig;

// ---------------------------------------------------------------------------
// Unit 1: Semantic version types and comparison
// ---------------------------------------------------------------------------

/// A parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    /// Major version number.
    pub major: u32,
    /// Minor version number.
    pub minor: u32,
    /// Patch version number.
    pub patch: u32,
    /// Optional pre-release label (e.g., "alpha.1", "rc.2").
    pub pre: Option<String>,
}

/// Errors from version parsing.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// The version string could not be parsed.
    #[error("invalid version string: {0}")]
    InvalidFormat(String),
}

impl SemVer {
    /// Parse a version string. Accepts "v1.2.3", "1.2.3", "1.2.3-rc.1".
    pub fn parse(_s: &str) -> Result<Self, VersionError> {
        todo!()
    }

    /// Returns true if `self` is strictly newer than `other`.
    /// Pre-release versions are considered older than the same version
    /// without a pre-release label (1.0.0-rc.1 < 1.0.0).
    pub fn is_newer_than(&self, _other: &SemVer) -> bool {
        todo!()
    }
}

impl std::fmt::Display for SemVer {
    /// Formats as "X.Y.Z" (no leading "v"), or "X.Y.Z-pre" if pre-release.
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

/// Compare the running version against a remote version.
/// Returns `Some(remote)` if remote is strictly newer, `None` otherwise.
pub fn check_newer(_current: &SemVer, _remote: &SemVer) -> Option<SemVer> {
    todo!()
}

// ---------------------------------------------------------------------------
// Unit 2: Install method detection
// ---------------------------------------------------------------------------

/// How the user installed Veil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    /// Installed via Homebrew.
    Homebrew,
    /// Installed via cargo install.
    Cargo,
    /// Installed via AUR helper (e.g., paru, yay).
    Aur,
    /// Installed via winget.
    Winget,
    /// Installed via scoop.
    Scoop,
    /// Installed via Nix.
    Nix,
    /// Unknown install method (manual build, pre-built binary, etc.).
    Unknown,
}

impl InstallMethod {
    /// Detect the install method by examining the current binary's path
    /// and environment.
    pub fn detect() -> Self {
        todo!()
    }

    /// Detect from a given binary path (testable without `current_exe()`).
    pub fn detect_from_path(_path: &Path) -> Self {
        todo!()
    }

    /// Return the upgrade command for this install method.
    /// Returns `None` for `Unknown` (we don't know how to upgrade).
    pub fn upgrade_command(&self) -> Option<&'static str> {
        todo!()
    }
}

impl std::fmt::Display for InstallMethod {
    /// Human-readable name: "Homebrew", "cargo", "AUR", etc.
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

// ---------------------------------------------------------------------------
// Unit 4: GitHub release version fetcher
// ---------------------------------------------------------------------------

/// Errors that can occur during update checking.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Network request failed.
    #[error("failed to fetch latest version: {0}")]
    FetchFailed(String),

    /// GitHub API response could not be parsed.
    #[error("failed to parse GitHub API response: {0}")]
    ParseFailed(String),

    /// Rate limited by GitHub API.
    #[error("GitHub API rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimited {
        /// Seconds until the rate limit resets.
        retry_after_secs: u64,
    },

    /// Version string in the release tag could not be parsed.
    #[error("invalid version in release tag: {0}")]
    InvalidVersion(#[from] VersionError),
}

/// Result of an update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// A newer version is available.
    Available {
        /// The latest version.
        latest: SemVer,
        /// The running version.
        current: SemVer,
        /// Detected install method.
        install_method: InstallMethod,
    },
    /// Already running the latest version.
    UpToDate {
        /// The current/latest version.
        version: SemVer,
    },
}

/// Trait for fetching the latest version tag from a remote source.
/// Abstracted for testing -- production uses HTTP, tests use a mock.
#[cfg_attr(test, mockall::automock)]
pub trait VersionFetcher: Send + Sync {
    /// Fetch the latest release version tag.
    /// Returns the tag string (e.g., "v0.2.0").
    fn fetch_latest_tag(&self) -> Result<String, UpdateError>;
}

/// Check for updates using the provided fetcher.
/// This is the pure logic function -- no I/O, fully testable.
pub fn check_for_update(
    _current: &SemVer,
    _fetcher: &dyn VersionFetcher,
    _install_method: &InstallMethod,
) -> Result<UpdateStatus, UpdateError> {
    todo!()
}

/// Production implementation that calls the GitHub Releases API.
#[allow(dead_code)]
pub struct GitHubVersionFetcher {
    /// GitHub repo in "owner/repo" format.
    repo: String,
    /// User-Agent header (GitHub API requires one).
    user_agent: String,
}

impl GitHubVersionFetcher {
    /// Create a new fetcher for the given repo.
    pub fn new(_repo: String) -> Self {
        todo!()
    }
}

impl VersionFetcher for GitHubVersionFetcher {
    fn fetch_latest_tag(&self) -> Result<String, UpdateError> {
        todo!()
    }
}

// ---------------------------------------------------------------------------
// Unit 5: Last-check timestamp persistence
// ---------------------------------------------------------------------------

/// Persistent state for the update checker (stored as a small JSON file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateState {
    /// When the last update check was performed (Unix timestamp).
    pub last_check_epoch: i64,
    /// The latest version found at last check (if any).
    pub latest_version: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for UpdateState {
    fn default() -> Self {
        Self { last_check_epoch: 0, latest_version: None }
    }
}

impl UpdateState {
    /// Load state from the default path, or return a default (never checked).
    pub fn load() -> Self {
        todo!()
    }

    /// Load state from a specific path (for testing).
    pub fn load_from(_path: &Path) -> Self {
        todo!()
    }

    /// Save state to the default path.
    pub fn save(&self) -> Result<(), std::io::Error> {
        todo!()
    }

    /// Save state to a specific path (for testing).
    pub fn save_to(&self, _path: &Path) -> Result<(), std::io::Error> {
        todo!()
    }

    /// Check if enough time has elapsed since the last check.
    pub fn should_check(&self, _interval_hours: u32) -> bool {
        todo!()
    }

    /// Default path: `~/.local/share/veil/update-state.json`
    /// (or platform equivalent via `dirs::data_local_dir()`).
    pub fn default_path() -> Option<PathBuf> {
        todo!()
    }
}

// ---------------------------------------------------------------------------
// Unit 6: Update checker orchestrator
// ---------------------------------------------------------------------------

/// The full update check result, ready for the UI layer.
#[derive(Debug, Clone)]
pub struct UpdateNotification {
    /// The available version.
    pub latest_version: SemVer,
    /// The running version.
    pub current_version: SemVer,
    /// Human-readable upgrade instruction (e.g., "run `brew upgrade veil`").
    /// `None` if install method is Unknown.
    pub upgrade_instruction: Option<String>,
    /// Detected install method.
    pub install_method: InstallMethod,
}

impl UpdateNotification {
    /// Format the notification message for the sidebar footer.
    /// Example: "Veil 0.2.0 available -- run `brew upgrade veil`"
    /// If install method is Unknown: "Veil 0.2.0 available"
    pub fn message(&self) -> String {
        todo!()
    }
}

/// Run the update check flow. Designed to be called from a background thread.
///
/// Steps:
/// 1. If `config.check_on_startup` is false, return `Ok(None)`.
/// 2. Read `UpdateState` from `state_path`.
/// 3. If `should_check` is false (within interval), return `Ok(None)`.
/// 4. Call fetcher to get latest tag.
/// 5. Parse and compare against current version.
/// 6. Update and save `UpdateState`.
/// 7. Return `UpdateNotification` if newer version available.
pub fn run_update_check(
    _current_version: &SemVer,
    _config: &UpdatesConfig,
    _fetcher: &dyn VersionFetcher,
    _install_method: &InstallMethod,
    _state_path: &Path,
) -> Result<Option<UpdateNotification>, UpdateError> {
    todo!()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ---------------------------------------------------------------
    // Unit 1: SemVer parsing, comparison, display
    // ---------------------------------------------------------------

    mod semver_parsing {
        use super::*;

        #[test]
        fn parse_basic_version() {
            let v = SemVer::parse("1.2.3").expect("should parse");
            assert_eq!(v, SemVer { major: 1, minor: 2, patch: 3, pre: None });
        }

        #[test]
        fn parse_leading_v_stripped() {
            let v = SemVer::parse("v1.2.3").expect("should parse");
            assert_eq!(v, SemVer { major: 1, minor: 2, patch: 3, pre: None });
        }

        #[test]
        fn parse_zero_version() {
            let v = SemVer::parse("0.1.0").expect("should parse");
            assert_eq!(v, SemVer { major: 0, minor: 1, patch: 0, pre: None });
        }

        #[test]
        fn parse_alpha_pre_release() {
            let v = SemVer::parse("1.2.3-alpha.1").expect("should parse");
            assert_eq!(
                v,
                SemVer { major: 1, minor: 2, patch: 3, pre: Some("alpha.1".to_string()) }
            );
        }

        #[test]
        fn parse_rc_pre_release() {
            let v = SemVer::parse("1.2.3-rc.2").expect("should parse");
            assert_eq!(v, SemVer { major: 1, minor: 2, patch: 3, pre: Some("rc.2".to_string()) });
        }

        #[test]
        fn parse_empty_string_returns_error() {
            let err = SemVer::parse("").unwrap_err();
            assert!(matches!(err, VersionError::InvalidFormat(_)));
        }

        #[test]
        fn parse_abc_returns_error() {
            let err = SemVer::parse("abc").unwrap_err();
            assert!(matches!(err, VersionError::InvalidFormat(_)));
        }

        #[test]
        fn parse_missing_patch_returns_error() {
            let err = SemVer::parse("1.2").unwrap_err();
            assert!(matches!(err, VersionError::InvalidFormat(_)));
        }

        #[test]
        fn parse_too_many_components_returns_error() {
            let err = SemVer::parse("1.2.3.4").unwrap_err();
            assert!(matches!(err, VersionError::InvalidFormat(_)));
        }

        #[test]
        fn parse_bare_v_returns_error() {
            let err = SemVer::parse("v").unwrap_err();
            assert!(matches!(err, VersionError::InvalidFormat(_)));
        }
    }

    mod semver_comparison {
        use super::*;

        #[test]
        fn minor_bump_is_newer() {
            let a = SemVer { major: 1, minor: 1, patch: 0, pre: None };
            let b = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            assert!(a.is_newer_than(&b));
        }

        #[test]
        fn patch_bump_is_newer() {
            let a = SemVer { major: 1, minor: 0, patch: 1, pre: None };
            let b = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            assert!(a.is_newer_than(&b));
        }

        #[test]
        fn major_bump_is_newer() {
            let a = SemVer { major: 2, minor: 0, patch: 0, pre: None };
            let b = SemVer { major: 1, minor: 9, patch: 9, pre: None };
            assert!(a.is_newer_than(&b));
        }

        #[test]
        fn equal_versions_not_newer() {
            let a = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            let b = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            assert!(!a.is_newer_than(&b));
        }

        #[test]
        fn older_version_not_newer() {
            let a = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            let b = SemVer { major: 1, minor: 1, patch: 0, pre: None };
            assert!(!a.is_newer_than(&b));
        }

        #[test]
        fn release_newer_than_pre_release() {
            let release = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            let pre = SemVer { major: 1, minor: 0, patch: 0, pre: Some("rc.1".to_string()) };
            assert!(release.is_newer_than(&pre));
        }

        #[test]
        fn pre_release_not_newer_than_release() {
            let pre = SemVer { major: 1, minor: 0, patch: 0, pre: Some("rc.2".to_string()) };
            let release = SemVer { major: 1, minor: 0, patch: 0, pre: None };
            assert!(!pre.is_newer_than(&release));
        }

        #[test]
        fn pre_release_alpha_not_newer_than_beta() {
            // Both are pre-releases of the same version; lexicographic comparison.
            // "alpha.1" < "beta.1" lexicographically, so alpha is not newer than beta.
            let alpha = SemVer { major: 1, minor: 0, patch: 0, pre: Some("alpha.1".to_string()) };
            let beta = SemVer { major: 1, minor: 0, patch: 0, pre: Some("beta.1".to_string()) };
            assert!(!alpha.is_newer_than(&beta));
        }
    }

    mod semver_display {
        use super::*;

        #[test]
        fn display_without_pre_release() {
            let v = SemVer { major: 1, minor: 2, patch: 3, pre: None };
            assert_eq!(v.to_string(), "1.2.3");
        }

        #[test]
        fn display_with_pre_release() {
            let v = SemVer { major: 1, minor: 0, patch: 0, pre: Some("rc.1".to_string()) };
            assert_eq!(v.to_string(), "1.0.0-rc.1");
        }
    }

    mod check_newer_fn {
        use super::*;

        #[test]
        fn returns_some_when_remote_is_newer() {
            let current = SemVer::parse("0.1.0").unwrap();
            let remote = SemVer::parse("0.2.0").unwrap();
            let result = check_newer(&current, &remote);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), remote);
        }

        #[test]
        fn returns_none_when_equal() {
            let current = SemVer::parse("1.0.0").unwrap();
            let remote = SemVer::parse("1.0.0").unwrap();
            assert!(check_newer(&current, &remote).is_none());
        }

        #[test]
        fn returns_none_when_remote_is_older() {
            let current = SemVer::parse("1.1.0").unwrap();
            let remote = SemVer::parse("1.0.0").unwrap();
            assert!(check_newer(&current, &remote).is_none());
        }
    }

    mod semver_properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn irreflexivity(major in 0u32..100, minor in 0u32..100, patch in 0u32..100) {
                let v = SemVer { major, minor, patch, pre: None };
                assert!(!v.is_newer_than(&v));
            }

            #[test]
            fn asymmetry(
                a_major in 0u32..50, a_minor in 0u32..50, a_patch in 0u32..50,
                b_major in 0u32..50, b_minor in 0u32..50, b_patch in 0u32..50,
            ) {
                let a = SemVer { major: a_major, minor: a_minor, patch: a_patch, pre: None };
                let b = SemVer { major: b_major, minor: b_minor, patch: b_patch, pre: None };
                if a.is_newer_than(&b) {
                    assert!(!b.is_newer_than(&a), "if a > b then b must not be > a");
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // Unit 2: Install method detection
    // ---------------------------------------------------------------

    mod install_method_detection {
        use super::*;

        #[test]
        fn homebrew_cellar_path() {
            let path = PathBuf::from("/opt/homebrew/Cellar/veil/0.1.0/bin/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Homebrew);
        }

        #[test]
        fn homebrew_usr_local_cellar_path() {
            let path = PathBuf::from("/usr/local/Cellar/veil/0.1.0/bin/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Homebrew);
        }

        #[test]
        fn cargo_bin_path() {
            let path = PathBuf::from("/home/user/.cargo/bin/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Cargo);
        }

        #[test]
        fn nix_store_path() {
            let path = PathBuf::from("/nix/store/abc123-veil-0.1.0/bin/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Nix);
        }

        #[test]
        fn scoop_path_windows() {
            let path = PathBuf::from(r"C:\Users\user\scoop\apps\veil\current\veil.exe");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Scoop);
        }

        #[test]
        fn unknown_usr_local_bin() {
            let path = PathBuf::from("/usr/local/bin/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Unknown);
        }

        #[test]
        fn unknown_manual_build() {
            let path = PathBuf::from("/home/user/builds/veil/target/release/veil");
            assert_eq!(InstallMethod::detect_from_path(&path), InstallMethod::Unknown);
        }
    }

    mod install_method_upgrade_commands {
        use super::*;

        #[test]
        fn homebrew_upgrade_command() {
            assert_eq!(InstallMethod::Homebrew.upgrade_command(), Some("brew upgrade veil"));
        }

        #[test]
        fn cargo_upgrade_command() {
            assert_eq!(InstallMethod::Cargo.upgrade_command(), Some("cargo install veil"));
        }

        #[test]
        fn unknown_upgrade_command_is_none() {
            assert_eq!(InstallMethod::Unknown.upgrade_command(), None);
        }

        #[test]
        fn all_known_methods_have_upgrade_command() {
            let known = [
                InstallMethod::Homebrew,
                InstallMethod::Cargo,
                InstallMethod::Aur,
                InstallMethod::Winget,
                InstallMethod::Scoop,
                InstallMethod::Nix,
            ];
            for method in &known {
                assert!(
                    method.upgrade_command().is_some(),
                    "{method:?} should have an upgrade command"
                );
            }
        }
    }

    mod install_method_display {
        use super::*;

        #[test]
        fn homebrew_display() {
            assert_eq!(InstallMethod::Homebrew.to_string(), "Homebrew");
        }

        #[test]
        fn cargo_display() {
            assert_eq!(InstallMethod::Cargo.to_string(), "cargo");
        }

        #[test]
        fn unknown_display() {
            assert_eq!(InstallMethod::Unknown.to_string(), "unknown");
        }
    }

    mod install_method_detect_runtime {
        use super::*;

        #[test]
        fn detect_does_not_panic() {
            // Should never panic regardless of runtime environment.
            let _ = InstallMethod::detect();
        }
    }

    // ---------------------------------------------------------------
    // Unit 3: Update check configuration (in config/model.rs)
    // Tests here verify integration between update.rs types and config.
    // ---------------------------------------------------------------

    mod updates_config {
        use crate::config::{AppConfig, UpdatesConfig};

        #[test]
        fn default_check_on_startup_is_true() {
            let config = UpdatesConfig::default();
            assert!(config.check_on_startup);
        }

        #[test]
        fn default_check_interval_hours_is_24() {
            let config = UpdatesConfig::default();
            assert_eq!(config.check_interval_hours, 24);
        }

        #[test]
        fn app_config_default_updates_matches() {
            let config = AppConfig::default();
            assert_eq!(config.updates, UpdatesConfig::default());
        }

        #[test]
        fn empty_toml_still_has_updates_defaults() {
            let config: AppConfig = toml::from_str("").expect("empty TOML should parse");
            assert!(config.updates.check_on_startup);
            assert_eq!(config.updates.check_interval_hours, 24);
        }

        #[test]
        fn parse_check_on_startup_false() {
            let toml_str = "[updates]\ncheck_on_startup = false\n";
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert!(!config.updates.check_on_startup);
        }

        #[test]
        fn parse_check_interval_hours() {
            let toml_str = "[updates]\ncheck_interval_hours = 168\n";
            let config: AppConfig = toml::from_str(toml_str).expect("should parse");
            assert_eq!(config.updates.check_interval_hours, 168);
        }

        #[test]
        fn round_trip_serialization_preserves_values() {
            let mut config = AppConfig::default();
            config.updates.check_on_startup = false;
            config.updates.check_interval_hours = 72;
            let serialized = toml::to_string(&config).expect("serialize");
            let roundtrip: AppConfig = toml::from_str(&serialized).expect("deserialize");
            assert_eq!(roundtrip.updates, config.updates);
        }
    }

    mod updates_config_diffing {
        use crate::config::{AppConfig, ConfigDelta};

        #[test]
        fn changing_check_on_startup_sets_updates_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.updates.check_on_startup = false;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.updates_changed);
        }

        #[test]
        fn changing_check_interval_hours_sets_updates_changed() {
            let a = AppConfig::default();
            let mut b = a.clone();
            b.updates.check_interval_hours = 48;
            let delta = ConfigDelta::diff(&a, &b);
            assert!(delta.updates_changed);
        }

        #[test]
        fn unchanged_updates_produces_false() {
            let a = AppConfig::default();
            let b = a.clone();
            let delta = ConfigDelta::diff(&a, &b);
            assert!(!delta.updates_changed);
        }

        #[test]
        fn is_empty_returns_false_when_updates_changed() {
            let delta = ConfigDelta { updates_changed: true, ..ConfigDelta::default() };
            assert!(!delta.is_empty());
        }
    }

    mod updates_config_validation {
        use crate::config::{validate_config, AppConfig};

        #[test]
        fn check_interval_hours_zero_clamped_to_1() {
            let mut config = AppConfig::default();
            config.updates.check_interval_hours = 0;
            let (validated, warnings) = validate_config(config);
            assert_eq!(validated.updates.check_interval_hours, 1);
            assert!(warnings.iter().any(|w| w.field.contains("updates.check_interval_hours")));
        }
    }

    // ---------------------------------------------------------------
    // Unit 4: Version fetcher (using MockVersionFetcher)
    // ---------------------------------------------------------------

    mod check_for_update_tests {
        use super::*;

        #[test]
        fn newer_version_returns_available() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.2.0".to_string()));

            let result = check_for_update(&current, &mock, &InstallMethod::Homebrew).unwrap();
            match result {
                UpdateStatus::Available { latest, current: c, install_method } => {
                    assert_eq!(latest, SemVer::parse("0.2.0").unwrap());
                    assert_eq!(c, SemVer::parse("0.1.0").unwrap());
                    assert_eq!(install_method, InstallMethod::Homebrew);
                }
                UpdateStatus::UpToDate { .. } => {
                    panic!("expected Available, got UpToDate")
                }
            }
        }

        #[test]
        fn same_version_returns_up_to_date() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.1.0".to_string()));

            let result = check_for_update(&current, &mock, &InstallMethod::Cargo).unwrap();
            assert!(matches!(result, UpdateStatus::UpToDate { .. }));
        }

        #[test]
        fn current_newer_returns_up_to_date() {
            let current = SemVer::parse("0.2.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.1.0".to_string()));

            let result = check_for_update(&current, &mock, &InstallMethod::Unknown).unwrap();
            assert!(matches!(result, UpdateStatus::UpToDate { .. }));
        }

        #[test]
        fn fetch_failed_propagated() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag()
                .returning(|| Err(UpdateError::FetchFailed("network error".to_string())));

            let result = check_for_update(&current, &mock, &InstallMethod::Unknown);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), UpdateError::FetchFailed(_)));
        }

        #[test]
        fn rate_limited_propagated() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag()
                .returning(|| Err(UpdateError::RateLimited { retry_after_secs: 60 }));

            let result = check_for_update(&current, &mock, &InstallMethod::Unknown);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), UpdateError::RateLimited { .. }));
        }

        #[test]
        fn unparseable_version_returns_invalid_version() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("not-a-version".to_string()));

            let result = check_for_update(&current, &mock, &InstallMethod::Unknown);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), UpdateError::InvalidVersion(_)));
        }

        #[test]
        fn install_method_carried_through() {
            let current = SemVer::parse("0.1.0").unwrap();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.2.0".to_string()));

            let result = check_for_update(&current, &mock, &InstallMethod::Nix).unwrap();
            match result {
                UpdateStatus::Available { install_method, .. } => {
                    assert_eq!(install_method, InstallMethod::Nix);
                }
                UpdateStatus::UpToDate { .. } => panic!("expected Available"),
            }
        }
    }

    mod github_fetcher_construction {
        use super::*;

        #[test]
        fn new_does_not_panic() {
            let _fetcher = GitHubVersionFetcher::new("veil-term/veil".to_string());
        }
    }

    // ---------------------------------------------------------------
    // Unit 5: UpdateState persistence
    // ---------------------------------------------------------------

    mod update_state_tests {
        use super::*;

        #[test]
        fn load_from_nonexistent_returns_default() {
            let state = UpdateState::load_from(Path::new("/tmp/nonexistent-veil-state.json"));
            assert_eq!(state.last_check_epoch, 0);
            assert!(state.latest_version.is_none());
        }

        #[test]
        fn default_state_should_check() {
            let state = UpdateState::default();
            assert!(state.should_check(24));
        }

        #[test]
        fn save_and_load_round_trip() {
            let dir = TempDir::new().expect("create temp dir");
            let path = dir.path().join("update-state.json");

            let state = UpdateState { last_check_epoch: 1_700_000_000, latest_version: None };
            state.save_to(&path).expect("save should succeed");

            let loaded = UpdateState::load_from(&path);
            assert_eq!(loaded.last_check_epoch, 1_700_000_000);
            assert!(loaded.latest_version.is_none());
        }

        #[test]
        fn save_and_load_with_latest_version() {
            let dir = TempDir::new().expect("create temp dir");
            let path = dir.path().join("update-state.json");

            let state = UpdateState {
                last_check_epoch: 1_700_000_000,
                latest_version: Some("0.2.0".to_string()),
            };
            state.save_to(&path).expect("save should succeed");

            let loaded = UpdateState::load_from(&path);
            assert_eq!(loaded.latest_version, Some("0.2.0".to_string()));
        }

        #[test]
        fn should_check_returns_false_when_checked_recently() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .cast_signed();
            let state = UpdateState { last_check_epoch: now, latest_version: None };
            assert!(!state.should_check(24));
        }

        #[test]
        fn should_check_returns_true_when_interval_exceeded() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .cast_signed();
            let twenty_five_hours_ago = now - (25 * 3600);
            let state =
                UpdateState { last_check_epoch: twenty_five_hours_ago, latest_version: None };
            assert!(state.should_check(24));
        }

        #[test]
        fn should_check_returns_true_when_never_checked() {
            let state = UpdateState { last_check_epoch: 0, latest_version: None };
            assert!(state.should_check(24));
        }

        #[test]
        fn should_check_returns_false_when_within_interval() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .cast_signed();
            let twenty_three_hours_ago = now - (23 * 3600);
            let state =
                UpdateState { last_check_epoch: twenty_three_hours_ago, latest_version: None };
            assert!(!state.should_check(24));
        }

        #[test]
        fn load_from_corrupted_json_returns_default() {
            let dir = TempDir::new().expect("create temp dir");
            let path = dir.path().join("corrupted.json");
            std::fs::write(&path, "not valid json {{{").expect("write corrupted");

            let state = UpdateState::load_from(&path);
            assert_eq!(state.last_check_epoch, 0);
        }

        #[test]
        fn save_to_creates_parent_directories() {
            let dir = TempDir::new().expect("create temp dir");
            let nested_path = dir.path().join("a").join("b").join("c").join("state.json");

            let state = UpdateState { last_check_epoch: 123, latest_version: None };
            state.save_to(&nested_path).expect("save should create parent dirs");

            assert!(nested_path.exists());
        }
    }

    mod update_state_default_path {
        use super::*;

        #[test]
        fn default_path_returns_some() {
            // Should return Some on any platform where dirs::data_local_dir works.
            let path = UpdateState::default_path();
            if let Some(p) = &path {
                assert!(p.to_string_lossy().contains("veil"), "path should contain 'veil': {p:?}");
            }
            // If dirs::data_local_dir returns None, path can be None. Not a test failure.
        }
    }

    // ---------------------------------------------------------------
    // Unit 6: Update checker orchestrator and notification
    // ---------------------------------------------------------------

    mod notification_message {
        use super::*;

        #[test]
        fn homebrew_message_contains_brew_upgrade() {
            let n = UpdateNotification {
                latest_version: SemVer::parse("0.2.0").unwrap(),
                current_version: SemVer::parse("0.1.0").unwrap(),
                upgrade_instruction: Some("run `brew upgrade veil`".to_string()),
                install_method: InstallMethod::Homebrew,
            };
            let msg = n.message();
            assert!(msg.contains("brew upgrade veil"), "message: {msg}");
            assert!(msg.contains("0.2.0"), "message should contain version: {msg}");
        }

        #[test]
        fn cargo_message_contains_cargo_install() {
            let n = UpdateNotification {
                latest_version: SemVer::parse("0.2.0").unwrap(),
                current_version: SemVer::parse("0.1.0").unwrap(),
                upgrade_instruction: Some("run `cargo install veil`".to_string()),
                install_method: InstallMethod::Cargo,
            };
            let msg = n.message();
            assert!(msg.contains("cargo install veil"), "message: {msg}");
        }

        #[test]
        fn unknown_message_no_upgrade_command() {
            let n = UpdateNotification {
                latest_version: SemVer::parse("0.2.0").unwrap(),
                current_version: SemVer::parse("0.1.0").unwrap(),
                upgrade_instruction: None,
                install_method: InstallMethod::Unknown,
            };
            let msg = n.message();
            assert!(!msg.contains("run `"), "should not have upgrade cmd: {msg}");
            assert!(msg.contains("0.2.0"), "should contain version: {msg}");
        }

        #[test]
        fn message_always_contains_version() {
            let n = UpdateNotification {
                latest_version: SemVer::parse("1.5.3").unwrap(),
                current_version: SemVer::parse("1.0.0").unwrap(),
                upgrade_instruction: None,
                install_method: InstallMethod::Unknown,
            };
            let msg = n.message();
            assert!(msg.contains("1.5.3"), "should contain version: {msg}");
        }
    }

    mod run_update_check_tests {
        use super::*;
        use crate::config::UpdatesConfig;

        fn default_config() -> UpdatesConfig {
            UpdatesConfig { check_on_startup: true, check_interval_hours: 24 }
        }

        #[test]
        fn check_disabled_returns_none_without_fetching() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = UpdatesConfig { check_on_startup: false, check_interval_hours: 24 };
            // The mock will panic if fetch_latest_tag is called, verifying the short-circuit.
            let mock = MockVersionFetcher::new();

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");
            let result =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path)
                    .unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn never_checked_and_newer_returns_notification() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.2.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");
            let result =
                run_update_check(&current, &config, &mock, &InstallMethod::Homebrew, &state_path)
                    .unwrap();
            assert!(result.is_some());
            let notification = result.unwrap();
            assert_eq!(notification.latest_version, SemVer::parse("0.2.0").unwrap());
        }

        #[test]
        fn never_checked_and_same_version_returns_none() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.1.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");
            let result =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path)
                    .unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn recent_check_skips_fetch() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            // Mock should not be called; will panic if it is.
            let mock = MockVersionFetcher::new();

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");

            // Write a recent state
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .cast_signed();
            let recent_state = UpdateState {
                last_check_epoch: now - 3600, // 1 hour ago
                latest_version: None,
            };
            recent_state.save_to(&state_path).unwrap();

            let result =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path)
                    .unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn old_check_triggers_fetch() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().times(1).returning(|| Ok("v0.2.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");

            // Write an old state
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .cast_signed();
            let old_state = UpdateState {
                last_check_epoch: now - (25 * 3600), // 25 hours ago
                latest_version: None,
            };
            old_state.save_to(&state_path).unwrap();

            let result =
                run_update_check(&current, &config, &mock, &InstallMethod::Cargo, &state_path)
                    .unwrap();
            assert!(result.is_some());
        }

        #[test]
        fn never_checked_triggers_fetch() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().times(1).returning(|| Ok("v0.1.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");
            // Don't write any state file -- should default to epoch 0.
            let _result =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path);
            // The mock's times(1) assertion verifies fetch was called.
        }

        #[test]
        fn successful_check_writes_state() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.2.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");

            let _ =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path);

            assert!(state_path.exists(), "state file should be written after check");
            let saved = UpdateState::load_from(&state_path);
            assert!(saved.last_check_epoch > 0, "timestamp should be updated");
        }

        #[test]
        fn successful_check_writes_latest_version() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag().returning(|| Ok("v0.2.0".to_string()));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");

            let _ =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path);

            let saved = UpdateState::load_from(&state_path);
            assert_eq!(saved.latest_version, Some("0.2.0".to_string()));
        }

        #[test]
        fn failed_check_does_not_update_state() {
            let current = SemVer::parse("0.1.0").unwrap();
            let config = default_config();
            let mut mock = MockVersionFetcher::new();
            mock.expect_fetch_latest_tag()
                .returning(|| Err(UpdateError::FetchFailed("network down".to_string())));

            let dir = TempDir::new().unwrap();
            let state_path = dir.path().join("state.json");

            // Write initial state
            let initial_state =
                UpdateState { last_check_epoch: 42, latest_version: Some("0.0.1".to_string()) };
            initial_state.save_to(&state_path).unwrap();

            let _ =
                run_update_check(&current, &config, &mock, &InstallMethod::Unknown, &state_path);

            let saved = UpdateState::load_from(&state_path);
            assert_eq!(saved.last_check_epoch, 42, "state should not be updated on failure");
            assert_eq!(
                saved.latest_version,
                Some("0.0.1".to_string()),
                "latest_version should be preserved on failure"
            );
        }
    }

    // ---------------------------------------------------------------
    // Unit 6: StateUpdate::UpdateAvailable integration
    // ---------------------------------------------------------------

    mod state_update_integration {
        use super::*;
        use crate::message::{Channels, StateUpdate};

        #[test]
        fn update_available_can_be_constructed_and_matched() {
            let notification = UpdateNotification {
                latest_version: SemVer::parse("0.2.0").unwrap(),
                current_version: SemVer::parse("0.1.0").unwrap(),
                upgrade_instruction: Some("run `brew upgrade veil`".to_string()),
                install_method: InstallMethod::Homebrew,
            };
            let update = StateUpdate::UpdateAvailable(notification);
            match update {
                StateUpdate::UpdateAvailable(n) => {
                    assert_eq!(n.latest_version, SemVer::parse("0.2.0").unwrap());
                    assert_eq!(n.install_method, InstallMethod::Homebrew);
                }
                other => panic!("expected UpdateAvailable, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn update_available_round_trip_through_channel() {
            let channels = Channels::new(16);
            let Channels { state_tx, mut state_rx, .. } = channels;

            let notification = UpdateNotification {
                latest_version: SemVer::parse("0.3.0").unwrap(),
                current_version: SemVer::parse("0.2.0").unwrap(),
                upgrade_instruction: Some("run `cargo install veil`".to_string()),
                install_method: InstallMethod::Cargo,
            };

            state_tx
                .send(StateUpdate::UpdateAvailable(notification))
                .await
                .expect("send should succeed");

            let msg = state_rx.recv().await.expect("should receive message");
            match msg {
                StateUpdate::UpdateAvailable(n) => {
                    assert_eq!(n.latest_version, SemVer::parse("0.3.0").unwrap());
                    assert_eq!(n.current_version, SemVer::parse("0.2.0").unwrap());
                    assert_eq!(n.install_method, InstallMethod::Cargo);
                    assert_eq!(n.upgrade_instruction, Some("run `cargo install veil`".to_string()));
                }
                other => panic!("expected UpdateAvailable, got: {other:?}"),
            }
        }
    }
}

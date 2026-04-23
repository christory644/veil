# VEI-22: Distribution — Package Managers + Update Notifications

## Context

Veil needs a distribution strategy and an update notification system so users can install via their preferred package manager and stay informed when new versions are available. This task covers three concerns:

1. **Update notification system** (testable Rust code in `veil-core`) — on startup, check the GitHub API for the latest release tag, compare against the running version, and surface a notification in the sidebar footer if a newer version exists. The notification adapts its message to the user's detected install method (e.g., "run `brew upgrade veil`" vs. "run `cargo install veil`").

2. **Cargo publish preparation** (workspace metadata) — ensure all `Cargo.toml` files have the metadata required for `cargo publish` (`description`, `homepage`, `categories`, `keywords`, `readme`, `license`, `repository`). This is testable: a build-time or CI check can verify the metadata is present.

3. **Package manager manifests and CI workflows** (configuration files) — Homebrew formula, AUR PKGBUILD, winget manifest, scoop bucket, and GitHub Actions release workflow. These are config/manifest files, not Rust code, and are mentioned here for completeness but are not TDD targets.

### Why this matters

Without update notifications, users have no way to know when a new version is available. Without package manager distribution, users can only build from source. The update notification system is privacy-respecting (no telemetry, just a single GitHub API call) and fully configurable (can be disabled in config).

### Scope boundaries

- **In scope**: Update check logic, version comparison, install method detection, config integration, `StateUpdate` variant, cargo publish metadata, package manager manifests (as config artifacts), GitHub Actions release workflow skeleton.
- **Out of scope**: Auto-update (downloading and replacing the binary), pre-built binary hosting infrastructure, signing/notarization. These are P3/future work per the PRD.

## Implementation Units

### Unit 1: Semantic version types and comparison

Define version types that can parse semver strings (like `v0.2.0` or `0.2.0`) and compare them to determine if an update is available.

**Types:**

```rust
// crates/veil-core/src/update.rs

/// A parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Optional pre-release label (e.g., "alpha.1", "rc.2").
    pub pre: Option<String>,
}

/// Errors from version parsing.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("invalid version string: {0}")]
    InvalidFormat(String),
}

impl SemVer {
    /// Parse a version string. Accepts "v1.2.3", "1.2.3", "1.2.3-rc.1".
    pub fn parse(s: &str) -> Result<Self, VersionError>;

    /// Returns true if `self` is strictly newer than `other`.
    /// Pre-release versions are considered older than the same version
    /// without a pre-release label (1.0.0-rc.1 < 1.0.0).
    pub fn is_newer_than(&self, other: &SemVer) -> bool;
}

impl std::fmt::Display for SemVer {
    /// Formats as "X.Y.Z" (no leading "v"), or "X.Y.Z-pre" if pre-release.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}

/// Compare the running version against a remote version.
/// Returns `Some(remote)` if remote is strictly newer, `None` otherwise.
pub fn check_newer(current: &SemVer, remote: &SemVer) -> Option<SemVer>;
```

**Files:**
- `crates/veil-core/src/update.rs` (new)
- `crates/veil-core/src/lib.rs` (add `pub mod update;`)

**Tests:**

*Parsing happy path:*
- `"1.2.3"` parses to `SemVer { major: 1, minor: 2, patch: 3, pre: None }`
- `"v1.2.3"` parses (leading "v" stripped)
- `"0.1.0"` parses
- `"1.2.3-alpha.1"` parses with `pre: Some("alpha.1")`
- `"1.2.3-rc.2"` parses with `pre: Some("rc.2")`

*Parsing error cases:*
- `""` returns `VersionError::InvalidFormat`
- `"abc"` returns `VersionError::InvalidFormat`
- `"1.2"` returns `VersionError::InvalidFormat` (missing patch)
- `"1.2.3.4"` returns `VersionError::InvalidFormat` (too many components)
- `"v"` returns `VersionError::InvalidFormat`

*Comparison:*
- `1.1.0` is newer than `1.0.0`
- `1.0.1` is newer than `1.0.0`
- `2.0.0` is newer than `1.9.9`
- `1.0.0` is NOT newer than `1.0.0` (equal)
- `1.0.0` is NOT newer than `1.1.0` (older)
- `1.0.0` is newer than `1.0.0-rc.1` (release > pre-release)
- `1.0.0-rc.2` is NOT newer than `1.0.0` (pre-release < release)
- `1.0.0-alpha.1` is NOT newer than `1.0.0-beta.1` (pre-release comparison follows semver 2.0.0 §11, both are older than 1.0.0)
- `1.0.0-beta` is newer than `1.0.0-alpha` (alphanumeric identifiers compared lexically)
- `1.0.0-2` is newer than `1.0.0-1` (numeric identifiers compared as integers)
- `1.0.0-11` is newer than `1.0.0-2` (numeric, not lexicographic: 11 > 2)
- `1.0.0-alpha.2` is newer than `1.0.0-alpha.1` (numeric suffix compared as integer)
- `1.0.0-alpha` is newer than `1.0.0-1` (alphanumeric > numeric per semver §11)
- `1.0.0-alpha.1` is newer than `1.0.0-alpha` (more identifiers = higher precedence when shared prefix is equal)

*Display:*
- `SemVer { 1, 2, 3, None }` displays as `"1.2.3"`
- `SemVer { 1, 0, 0, Some("rc.1") }` displays as `"1.0.0-rc.1"`

*`check_newer`:*
- Returns `Some(remote)` when remote is newer
- Returns `None` when remote equals current
- Returns `None` when remote is older

*Property-based:*
- For any version V, `V.is_newer_than(V)` is false (irreflexivity)
- For any versions A < B, `B.is_newer_than(A)` is true and `A.is_newer_than(B)` is false (asymmetry)

### Unit 2: Install method detection

Detect how the user installed Veil so the update notification can suggest the correct upgrade command. Detection is heuristic: check the binary's path and known package manager indicators.

**Types:**

```rust
// crates/veil-core/src/update.rs

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
    ///
    /// Heuristics (checked in order):
    /// 1. Binary path contains `/Cellar/` or `/homebrew/` -> Homebrew
    /// 2. Binary path contains `/.cargo/bin/` -> Cargo
    /// 3. Binary path contains `/nix/store/` -> Nix
    /// 4. Binary path contains `/scoop/` (case-insensitive) -> Scoop
    /// 5. (Windows) Binary path under winget package directory -> Winget
    /// 6. (Linux) Binary is `/usr/bin/veil` and pacman db exists -> Aur
    /// 7. Otherwise -> Unknown
    pub fn detect() -> Self;

    /// Detect from a given binary path (testable without `current_exe()`).
    pub fn detect_from_path(path: &std::path::Path) -> Self;

    /// Return the upgrade command for this install method.
    /// Returns `None` for `Unknown` (we don't know how to upgrade).
    pub fn upgrade_command(&self) -> Option<&'static str>;
}

impl std::fmt::Display for InstallMethod {
    /// Human-readable name: "Homebrew", "cargo", "AUR", etc.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

**Upgrade commands:**

| Method | Command |
|--------|---------|
| Homebrew | `brew upgrade veil` |
| Cargo | `cargo install veil` |
| Aur | `yay -Syu veil  (or your preferred AUR helper)` |
| Winget | `winget upgrade veil` |
| Scoop | `scoop update veil` |
| Nix | `nix profile upgrade veil` |
| Unknown | `None` |

**Files:**
- `crates/veil-core/src/update.rs` (same file as Unit 1)

**Tests:**

*Path-based detection:*
- Path `/opt/homebrew/Cellar/veil/0.1.0/bin/veil` -> `Homebrew`
- Path `/usr/local/Cellar/veil/0.1.0/bin/veil` -> `Homebrew`
- Path `/home/user/.cargo/bin/veil` -> `Cargo`
- Path `/nix/store/abc123-veil-0.1.0/bin/veil` -> `Nix`
- Path `C:\Users\user\scoop\apps\veil\current\veil.exe` -> `Scoop`
- Path `/usr/local/bin/veil` -> `Unknown`
- Path `/home/user/builds/veil/target/release/veil` -> `Unknown`

*Upgrade commands:*
- `Homebrew.upgrade_command()` returns `Some("brew upgrade veil")`
- `Cargo.upgrade_command()` returns `Some("cargo install veil")`
- `Unknown.upgrade_command()` returns `None`
- Every non-Unknown variant returns `Some(_)`

*Display:*
- `Homebrew` displays as `"Homebrew"`
- `Cargo` displays as `"cargo"`
- `Unknown` displays as `"unknown"`

*`detect()` does not panic:*
- Calling `InstallMethod::detect()` does not panic regardless of environment (may return any variant)

### Unit 3: Update check configuration

Add an `[updates]` section to `AppConfig` so users can disable update checks and control check frequency. Integrate with the existing config system.

**Types:**

```rust
// crates/veil-core/src/config/model.rs (add to AppConfig)

/// `[updates]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    /// Whether to check for updates on startup. Default: true.
    pub check_on_startup: bool,
    /// Minimum interval between update checks, in hours. Default: 24.
    /// Prevents excessive API calls when Veil is launched frequently.
    pub check_interval_hours: u32,
}
```

**Defaults:**

| Field | Default |
|-------|---------|
| `check_on_startup` | `true` |
| `check_interval_hours` | `24` |

**Config TOML example:**

```toml
[updates]
check_on_startup = false       # disable update checks entirely
check_interval_hours = 168     # check at most once per week
```

**Changes to existing types:**

- Add `pub updates: UpdatesConfig` field to `AppConfig`
- Add `pub updates_changed: bool` field to `ConfigDelta`
- Update `ConfigDelta::diff()` to compare the `updates` section
- Update `ConfigDelta::is_empty()` to include `updates_changed`
- Update config validation: `check_interval_hours` must be >= 1 (clamp with warning)

**Files:**
- `crates/veil-core/src/config/model.rs` (add `UpdatesConfig`, add field to `AppConfig`)
- `crates/veil-core/src/config/diff.rs` (add `updates_changed` to `ConfigDelta`)
- `crates/veil-core/src/config/parse.rs` (add validation for `check_interval_hours`)

**Tests:**

*Defaults:*
- `UpdatesConfig::default()` has `check_on_startup: true` and `check_interval_hours: 24`
- `AppConfig::default().updates` matches `UpdatesConfig::default()`

*Serde:*
- Empty TOML still deserializes to `AppConfig::default()` (updates section gets defaults)
- `[updates]\ncheck_on_startup = false` parses correctly
- `[updates]\ncheck_interval_hours = 168` parses correctly
- Round-trip serialization preserves values

*Diffing:*
- Changing `check_on_startup` sets `updates_changed = true`
- Changing `check_interval_hours` sets `updates_changed = true`
- Unchanged updates section produces `updates_changed = false`
- `ConfigDelta::is_empty()` returns `false` when `updates_changed = true`

*Validation:*
- `check_interval_hours = 0` is clamped to 1 with a warning

### Unit 4: GitHub release version fetcher

An async function that fetches the latest release tag from the GitHub API. Non-blocking, rate-limit aware, and designed for testability via a trait for the HTTP client.

**Types:**

```rust
// crates/veil-core/src/update.rs

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
/// Abstracted for testing — production uses HTTP, tests use a mock.
#[cfg_attr(test, mockall::automock)]
pub trait VersionFetcher: Send + Sync {
    /// Fetch the latest release version tag.
    /// Returns the tag string (e.g., "v0.2.0").
    fn fetch_latest_tag(&self) -> Result<String, UpdateError>;
}

/// Check for updates using the provided fetcher.
/// This is the pure logic function — no I/O, fully testable.
pub fn check_for_update(
    current: &SemVer,
    fetcher: &dyn VersionFetcher,
    install_method: &InstallMethod,
) -> Result<UpdateStatus, UpdateError>;

/// Production implementation that calls the GitHub Releases API.
/// Uses `https://api.github.com/repos/veil-term/veil/releases/latest`.
pub struct GitHubVersionFetcher {
    /// GitHub repo in "owner/repo" format.
    repo: String,
    /// User-Agent header (GitHub API requires one).
    user_agent: String,
}

impl GitHubVersionFetcher {
    pub fn new(repo: String) -> Self;
}

impl VersionFetcher for GitHubVersionFetcher {
    fn fetch_latest_tag(&self) -> Result<String, UpdateError>;
}
```

**GitHub API details:**
- Endpoint: `GET https://api.github.com/repos/veil-term/veil/releases/latest`
- No authentication required for public repos (60 requests/hour rate limit)
- Response JSON field: `.tag_name` contains the version string
- User-Agent header: `veil/{current_version}` (GitHub requires a User-Agent)
- Timeout: 5 seconds (don't block startup on slow networks)

**HTTP client choice:** Use `ureq` (blocking, minimal, no async runtime dependency) since this runs in a background thread, not the async runtime. The version check is a single GET request — a full async HTTP client (reqwest) would be overkill.

**Files:**
- `crates/veil-core/src/update.rs`
- `crates/veil-core/Cargo.toml` (add `ureq` and `serde_json` dependencies)

**Tests (using `MockVersionFetcher`):**

*`check_for_update` happy path:*
- Fetcher returns "v0.2.0", current is "0.1.0" -> `UpdateStatus::Available`
- Fetcher returns "v0.1.0", current is "0.1.0" -> `UpdateStatus::UpToDate`
- Fetcher returns "v0.1.0", current is "0.2.0" -> `UpdateStatus::UpToDate` (current is newer)

*`check_for_update` error cases:*
- Fetcher returns `FetchFailed` -> error propagated
- Fetcher returns `RateLimited` -> error propagated
- Fetcher returns unparseable version -> `InvalidVersion` error

*`UpdateStatus` carries correct install method:*
- When update is available, `install_method` in `Available` matches what was passed in

*`GitHubVersionFetcher` construction:*
- `new("veil-term/veil")` does not panic
- (Integration test, optional) Actually hits GitHub API and parses response — gated behind `#[ignore]` or a feature flag

### Unit 5: Last-check timestamp persistence

Track when the last update check occurred so we respect the `check_interval_hours` config. Store a small state file alongside the config.

**Types:**

```rust
// crates/veil-core/src/update.rs

/// Persistent state for the update checker (stored as a small JSON file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateState {
    /// When the last update check was performed (Unix timestamp).
    pub last_check_epoch: i64,
    /// The latest version found at last check (if any).
    pub latest_version: Option<String>,
}

impl UpdateState {
    /// Load state from the default path, or return a default (never checked).
    pub fn load() -> Self;

    /// Load state from a specific path (for testing).
    pub fn load_from(path: &std::path::Path) -> Self;

    /// Save state to the default path.
    pub fn save(&self) -> Result<(), std::io::Error>;

    /// Save state to a specific path (for testing).
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), std::io::Error>;

    /// Check if enough time has elapsed since the last check.
    pub fn should_check(&self, interval_hours: u32) -> bool;

    /// Default path: `~/.local/share/veil/update-state.json`
    /// (or platform equivalent via `dirs::data_local_dir()`).
    pub fn default_path() -> Option<std::path::PathBuf>;
}
```

**Files:**
- `crates/veil-core/src/update.rs`

**Tests:**

*Default state:*
- `UpdateState::load_from` on nonexistent file returns default with `last_check_epoch: 0`
- Default state `should_check(24)` returns `true` (never checked)

*Persistence round-trip:*
- Save state to tempfile, load from same file, values match
- Save with `latest_version: Some("0.2.0")`, load, value preserved

*`should_check`:*
- State with `last_check_epoch` = now returns `false` for `interval_hours = 24`
- State with `last_check_epoch` = 25 hours ago returns `true` for `interval_hours = 24`
- State with `last_check_epoch` = 0 (never checked) returns `true`
- State with `last_check_epoch` = 23 hours ago returns `false` for `interval_hours = 24`

*Error handling:*
- `load_from` with corrupted JSON returns default (does not panic)
- `save_to` creates parent directories if needed

### Unit 6: Update checker orchestrator and `StateUpdate` integration

Coordinate the full update check flow: read config, check interval, fetch if needed, compare versions, and send a `StateUpdate` to `AppState`. This runs as a fire-once background task on startup.

**Types:**

```rust
// crates/veil-core/src/update.rs

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
    pub fn message(&self) -> String;
}

/// Run the update check flow. Designed to be called from a background thread.
///
/// Steps:
/// 1. Read UpdateState from disk
/// 2. If `should_check` is false (within interval), return early with None
/// 3. Call fetcher to get latest tag
/// 4. Parse and compare against current version
/// 5. Update and save UpdateState
/// 6. Return UpdateNotification if newer version available
pub fn run_update_check(
    current_version: &SemVer,
    config: &UpdatesConfig,
    fetcher: &dyn VersionFetcher,
    install_method: &InstallMethod,
    state_path: &std::path::Path,
) -> Result<Option<UpdateNotification>, UpdateError>;
```

**`StateUpdate` variant:**

Add to `crates/veil-core/src/message.rs`:

```rust
/// An update is available.
UpdateAvailable(UpdateNotification),
```

**Files:**
- `crates/veil-core/src/update.rs`
- `crates/veil-core/src/message.rs` (add `UpdateAvailable` variant)

**Tests (all using `MockVersionFetcher` and tempdir for state):**

*Full flow happy path:*
- Config has `check_on_startup: true`, never checked before, fetcher returns newer version -> returns `Some(UpdateNotification)`
- Config has `check_on_startup: true`, never checked before, fetcher returns same version -> returns `None`
- Config has `check_on_startup: false` -> function returns `None` without calling fetcher

*Interval throttling:*
- Last check was 1 hour ago, interval is 24 hours -> fetcher NOT called, returns `None`
- Last check was 25 hours ago, interval is 24 hours -> fetcher called
- Last check epoch is 0 (never checked) -> fetcher called

*State persistence:*
- After successful check, state file is written with updated timestamp
- After successful check showing new version, `latest_version` is written
- After failed check (network error), state file is NOT updated (preserves previous)

*Notification message formatting:*
- Homebrew install: message contains "brew upgrade veil"
- Cargo install: message contains "cargo install veil"
- Unknown install: message does NOT contain an upgrade command
- All messages contain the version number

*`StateUpdate::UpdateAvailable` integration:*
- Can be constructed and pattern-matched
- Round-trip through mpsc channel preserves notification data

### Unit 7: Cargo publish metadata (workspace Cargo.toml)

Ensure all workspace crate `Cargo.toml` files have the metadata required for `cargo publish`. The binary crate (`veil`) must be publishable; library crates should also have correct metadata for potential future publishing.

**Changes to `Cargo.toml` files:**

Add to `[workspace.package]` in the root `Cargo.toml`:

```toml
description = "Cross-platform, GPU-accelerated terminal workspace manager for AI coding agents"
homepage = "https://github.com/veil-term/veil"
keywords = ["terminal", "workspace", "ai", "gpu", "multiplexer"]
categories = ["command-line-utilities", "development-tools"]
readme = "README.md"
```

These are inherited by all workspace crates via `description.workspace = true`, etc.

Individual crates that should NOT be published (internal-only) should have `publish = false` in their `[package]` section:
- `veil-scaffold-tests`
- `veil-e2e`

**Files:**
- `Cargo.toml` (root workspace)
- `crates/veil/Cargo.toml` (add `description.workspace = true`, etc.)
- `crates/veil-core/Cargo.toml` (same)
- All other crate `Cargo.toml` files
- `crates/veil-scaffold-tests/Cargo.toml` (add `publish = false`)
- `crates/veil-e2e/Cargo.toml` (add `publish = false`)

**Verification (not unit tests, but CI-checkable):**
- `cargo publish --dry-run -p veil` succeeds (or fails only on dependency issues, not metadata)
- All publishable crates have `description`, `license`, `repository`

### Unit 8: Package manager manifests (config files, not Rust code)

Create the package manager manifest files. These are configuration artifacts, not testable Rust code. They will be validated by CI (where possible) and by manual testing during the first release.

**Homebrew formula** (`packaging/homebrew/veil.rb`):

```ruby
class Veil < Formula
  desc "Cross-platform, GPU-accelerated terminal workspace manager for AI coding agents"
  homepage "https://github.com/veil-term/veil"
  license any_of: ["MIT", "Apache-2.0"]

  # Updated by release CI
  url "https://github.com/veil-term/veil/archive/refs/tags/v#{version}.tar.gz"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "veil", shell_output("#{bin}/veil --version")
  end
end
```

**AUR PKGBUILD** (`packaging/aur/PKGBUILD`):

Standard `PKGBUILD` that builds from source using `cargo build --release`.

**Winget manifest** (`packaging/winget/veil.yaml`):

Standard winget manifest YAML.

**Scoop manifest** (`packaging/scoop/veil.json`):

Standard scoop bucket JSON.

**Files:**
- `packaging/homebrew/veil.rb`
- `packaging/aur/PKGBUILD`
- `packaging/winget/veil.yaml`
- `packaging/scoop/veil.json`

**No Rust tests** — these are validated by their respective package manager tooling during release.

### Unit 9: GitHub Actions release workflow (CI config, not Rust code)

Create a release workflow that triggers on tag push, cross-compiles for all target platforms, and publishes release artifacts.

**Workflow** (`.github/workflows/release.yml`):

```yaml
name: Release
on:
  push:
    tags: ["v*"]

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: veil-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/veil*

  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
      - uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: veil-*/veil*
```

**Changelog generation:** Rely on GitHub's `generate_release_notes: true` which uses conventional commits and PR titles. A dedicated changelog generator (e.g., `git-cliff`) can be added later if needed.

**Version tagging strategy:** Standard semver tags (`v0.1.0`, `v0.2.0`, `v1.0.0`). Pre-release tags use `-` suffix (`v1.0.0-rc.1`). Tags are created manually or by a release script; the CI workflow triggers on tag push.

**Files:**
- `.github/workflows/release.yml`

**No Rust tests** — validated by GitHub Actions itself on first tag push.

## Acceptance Criteria

1. `cargo build -p veil-core` succeeds with the new `update` module
2. `cargo test -p veil-core` passes all update-related tests (version parsing, comparison, install detection, config, orchestrator)
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `SemVer::parse` correctly handles `"v1.2.3"`, `"1.2.3"`, and `"1.2.3-rc.1"` formats
6. `SemVer::is_newer_than` correctly compares major, minor, patch, and pre-release
7. `InstallMethod::detect_from_path` correctly identifies Homebrew, Cargo, Nix, Scoop, and Unknown installs from binary paths
8. `InstallMethod::upgrade_command` returns the correct command for each known install method
9. `AppConfig` has an `[updates]` section with `check_on_startup` (default true) and `check_interval_hours` (default 24)
10. `ConfigDelta::diff` detects changes to the `updates` section
11. `UpdateState` can persist and load the last-check timestamp from a JSON file
12. `run_update_check` respects `check_on_startup: false` by returning `None` without fetching
13. `run_update_check` respects `check_interval_hours` by skipping fetch when within the interval
14. `UpdateNotification::message()` adapts the upgrade command to the detected install method
15. `StateUpdate::UpdateAvailable` variant exists and can round-trip through the message channel
16. All workspace `Cargo.toml` files have `description`, `homepage`, `keywords`, `categories`, `readme`
17. Package manager manifests exist in `packaging/` directory
18. GitHub Actions release workflow exists at `.github/workflows/release.yml`

## Dependencies

**New crate dependencies:**

| Location | Dependency | Version | Reason |
|----------|-----------|---------|--------|
| workspace `Cargo.toml` | `ureq` | `3` | Blocking HTTP client for GitHub API version check |
| veil-core `Cargo.toml` | `ureq` | (workspace) | GitHub Releases API call |
| veil-core `Cargo.toml` | `serde_json` | (workspace, move to deps) | Parse GitHub API JSON response and update state file |

**Existing dependencies already available:**
- `serde` (with `derive`) -- already in veil-core dependencies
- `thiserror` -- already in veil-core dependencies
- `tracing` -- already in veil-core dependencies
- `chrono` -- already in veil-core dependencies (for timestamp comparison)
- `dirs` -- already in veil-core dependencies (for state file path)
- `tokio` (with `sync`) -- already in veil-core dependencies (for channel types)
- `tempfile` -- already in veil-core dev-dependencies (for tests)
- `mockall` -- already in veil-core dev-dependencies (for `MockVersionFetcher`)
- `proptest` -- already in veil-core dev-dependencies (for version comparison properties)

**No new external tools required.** The `ureq` crate is pure Rust with no native dependencies.

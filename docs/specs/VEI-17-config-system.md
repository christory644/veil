# VEI-17: Configuration System — TOML Config + Hot-Reload

## Context

Veil needs a configuration system that lets users customize terminal appearance, sidebar behavior, keybindings, and agent adapter settings via a TOML file. The system must support hot-reload so changes apply live without restarting the application.

The config system lives in `veil-core` (as noted in `AGENTS.md`: Config | veil-core | toml, serde, notify). It follows the actor model described in the system design: a config watcher runs as a background actor, monitoring the config file for changes. When the file changes, the watcher parses the new config, diffs it against the current state, and sends a `StateUpdate::ConfigReloaded` to `AppState` via the existing channel infrastructure.

### Why this matters

Without a config system, all appearance and behavior settings are hardcoded defaults. Users cannot customize fonts, themes, keybindings, or sidebar layout. Hot-reload is essential because terminal users expect live config changes (Ghostty, Alacritty, and WezTerm all support this).

### Scope boundaries

- **In scope**: Config data model, TOML parsing, defaults, file discovery, hot-reload watcher, config diffing, error reporting.
- **Out of scope**: Ghostty config import/parsing (VEI-35). The config struct will include a `ghostty.config_path` field but no parsing logic for the Ghostty format. Wiring into `AppState` is stubbed as a callback/channel interface; the actual integration happens when the app shell (VEI-10) is built.

## Implementation Units

### Unit 1: Config data model and defaults

Define the Rust structs that represent the full configuration, with serde deserialization and `Default` implementations providing sensible values for all fields.

**Types:**

```rust
// crates/veil-core/src/config.rs

/// Top-level application configuration, deserialized from config.toml.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub sidebar: SidebarConfig,
    pub terminal: TerminalConfig,
    pub conversations: ConversationsConfig,
    pub keybindings: KeybindingsConfig,
    pub ghostty: GhosttyConfig,
}

/// Theme preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

/// Workspace persistence behavior on exit/launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistenceMode {
    Restore,
    Fresh,
    Ask,
}

/// `[general]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub theme: ThemeMode,
    pub persistence: PersistenceMode,
}

/// Which sidebar tab to show by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultTab {
    Workspaces,
    Conversations,
}

/// `[sidebar]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarConfig {
    pub default_tab: DefaultTab,
    pub width: u32,
    pub visible: bool,
}

/// Font weight specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

/// `[terminal]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub scrollback_lines: u32,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub font_weight: FontWeight,
}

/// `[conversations]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationsConfig {
    pub adapters: Vec<String>,
}

/// `[keybindings]` section.
/// Keys are action names (e.g., "workspace_tab"), values are shortcut strings
/// (e.g., "ctrl+shift+w"). Unknown keys are ignored with a warning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub toggle_sidebar: Option<String>,
    pub workspace_tab: Option<String>,
    pub conversations_tab: Option<String>,
    pub new_workspace: Option<String>,
    pub close_workspace: Option<String>,
    pub split_horizontal: Option<String>,
    pub split_vertical: Option<String>,
    pub close_pane: Option<String>,
    pub focus_next_pane: Option<String>,
    pub focus_previous_pane: Option<String>,
    pub zoom_pane: Option<String>,
    pub focus_pane_left: Option<String>,
    pub focus_pane_right: Option<String>,
    pub focus_pane_up: Option<String>,
    pub focus_pane_down: Option<String>,
}

/// `[ghostty]` section.
/// Placeholder for Ghostty config import (VEI-35).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhosttyConfig {
    pub config_path: Option<String>,
}
```

**Defaults:**

| Section | Field | Default |
|---------|-------|---------|
| general | theme | `"dark"` |
| general | persistence | `"ask"` |
| sidebar | default_tab | `"workspaces"` |
| sidebar | width | `250` |
| sidebar | visible | `true` |
| terminal | scrollback_lines | `10000` |
| terminal | font_family | `None` (system default) |
| terminal | font_size | `14.0` |
| terminal | font_weight | `"regular"` |
| conversations | adapters | `["claude-code"]` |
| keybindings | (all) | `None` (uses hardcoded defaults from `KeybindingRegistry`) |
| ghostty | config_path | `None` |

**Files:**
- `crates/veil-core/src/config.rs` -- Config structs, Default impls, serde derives
- `crates/veil-core/src/lib.rs` -- Add `pub mod config;`

**Dependencies added to veil-core `Cargo.toml`:**
- `toml` (move from dev-dependencies to dependencies)
- `serde` already present

**Tests:**

*Defaults:*
- `AppConfig::default()` produces expected default values for all fields
- Each section struct's `Default` impl matches the documented defaults table
- `ThemeMode` default is `Dark`
- `PersistenceMode` default is `Ask`
- `DefaultTab` default is `Workspaces`
- `FontWeight` default is `Regular`

*Serde:*
- Empty TOML string `""` deserializes to `AppConfig::default()` (all defaults applied)
- Partial TOML (only `[general]` section) fills in defaults for missing sections
- Partial section (only `theme` in `[general]`) fills in defaults for missing fields
- `ThemeMode` serialization/deserialization round-trip for all variants
- `PersistenceMode` serialization/deserialization round-trip for all variants
- `DefaultTab` serialization/deserialization round-trip for all variants
- `FontWeight` serialization/deserialization round-trip for all variants
- `font_size` accepts integer values (e.g., `14` deserializes as `14.0`)
- `adapters` list with multiple entries preserves order
- `keybindings` with some fields set and others `None` deserializes correctly
- `ghostty.config_path` with `None` and with a path string both work

*Equality:*
- Two `AppConfig` instances with same values are equal (needed for diffing)
- Two `AppConfig` instances with different values are not equal

### Unit 2: Config file discovery

Implement platform-aware logic to locate the config file, following the priority order specified in the task.

**Functions:**

```rust
// crates/veil-core/src/config.rs (or config/discovery.rs)

/// Errors related to config file operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file at {path}: {source}")]
    ReadError { path: PathBuf, source: std::io::Error },

    #[error("failed to parse config at {path}: {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("config error at {path} line {line}, column {column}: {message}")]
    ParseErrorWithLocation {
        path: PathBuf,
        line: usize,
        column: usize,
        message: String,
    },

    #[error("failed to determine config directory")]
    NoConfigDir,
}

/// Search for the config file in platform-specific locations.
/// Returns the path to the first config file found, or None if
/// no config file exists (the app should use defaults).
///
/// Search order:
/// 1. `~/.config/veil/config.toml` (all platforms)
/// 2. macOS: `~/Library/Application Support/com.veil.app/config.toml`
/// 3. Windows: `%APPDATA%\veil\config.toml`
pub fn discover_config_path() -> Option<PathBuf>;

/// Return the primary config directory path (for creating defaults).
/// `~/.config/veil/` on all platforms.
pub fn primary_config_dir() -> Option<PathBuf>;

/// Load and parse a config file from the given path.
/// Returns the parsed config or a descriptive error.
pub fn load_config(path: &Path) -> Result<AppConfig, ConfigError>;

/// Load config from the discovered path, or return defaults if no file exists.
/// Returns (config, Option<path>) -- the path is None if using defaults.
pub fn load_or_default() -> (AppConfig, Option<PathBuf>);
```

**Files:**
- `crates/veil-core/src/config.rs` -- Add discovery functions alongside structs

**Dependencies:**
- `dirs` crate (already in workspace dependencies, needs adding to veil-core's `Cargo.toml`)

**Tests:**

*Discovery:*
- `primary_config_dir()` returns a path ending in `veil/` (or `veil` on Windows)
- `discover_config_path()` returns `None` when no config files exist (use tempdir)
- `discover_config_path()` finds file at `~/.config/veil/config.toml` location when it exists (use tempdir + env override or mock)

*Loading:*
- `load_config` with valid TOML returns parsed `AppConfig`
- `load_config` with invalid TOML returns `ConfigError::ParseError` with file path
- `load_config` with nonexistent path returns `ConfigError::ReadError`
- `load_config` with empty file returns `AppConfig::default()`
- `load_config` with partial TOML fills in defaults
- `load_or_default` returns defaults and `None` path when no file exists
- Parse errors include the file path for user-facing messages
- TOML parse errors include line/column when available

*Error messages:*
- `ConfigError::ParseError` Display includes path and message
- `ConfigError::ParseErrorWithLocation` Display includes path, line, column, message
- `ConfigError::ReadError` Display includes path and IO error
- `ConfigError::NoConfigDir` Display is informative

### Unit 3: TOML parsing with rich error reporting

Enhance the parsing to provide Rust-compiler-style error messages: line numbers, column positions, and contextual suggestions for common mistakes.

**Functions:**

```rust
// crates/veil-core/src/config.rs

/// Parse a TOML string into AppConfig with rich error reporting.
/// On failure, returns a ConfigError with line/column and a
/// human-readable message describing the problem and suggesting fixes.
pub fn parse_config(toml_str: &str, source_path: &Path) -> Result<AppConfig, ConfigError>;

/// Validate a parsed config for semantic correctness.
/// Catches issues that are syntactically valid TOML but semantically wrong:
/// - sidebar.width < 100 or > 1000 (warning, clamp to range)
/// - terminal.font_size < 6.0 or > 72.0 (warning, clamp to range)
/// - terminal.scrollback_lines == 0 (warning, set to 1)
/// - unknown adapter names in conversations.adapters (warning, skip)
/// Returns the validated config and a list of warnings.
pub fn validate_config(config: AppConfig) -> (AppConfig, Vec<ConfigWarning>);

/// A non-fatal issue found during config validation.
#[derive(Debug, Clone)]
pub struct ConfigWarning {
    pub field: String,
    pub message: String,
}
```

**Files:**
- `crates/veil-core/src/config.rs` -- Parsing and validation functions

**Tests:**

*Parsing happy path:*
- Full valid config parses all sections correctly
- Minimal config (single field) parses with defaults for everything else
- Comments in TOML are ignored
- Inline tables work (e.g., `keybindings = { workspace_tab = "ctrl+shift+w" }`)

*Parsing error cases:*
- Invalid TOML syntax (missing closing bracket) produces error with line number
- Wrong type for field (string where number expected) produces error with field name
- Unknown section name (e.g., `[nonexistent]`) is silently ignored (TOML serde default)
- Unknown field in a known section (e.g., `[sidebar] foo = "bar"`) is silently ignored

*Validation:*
- `sidebar.width = 50` is clamped to 100 with a warning
- `sidebar.width = 2000` is clamped to 1000 with a warning
- `terminal.font_size = 2.0` is clamped to 6.0 with a warning
- `terminal.font_size = 100.0` is clamped to 72.0 with a warning
- `terminal.scrollback_lines = 0` is set to 1 with a warning
- Valid values pass through without warnings
- Multiple validation issues produce multiple warnings

### Unit 4: Config diffing

Compare two `AppConfig` instances and produce a `ConfigDelta` describing what changed. This is used by the hot-reload watcher to determine which parts of the UI need updating.

**Types:**

```rust
// crates/veil-core/src/config.rs

/// Describes what changed between two config versions.
/// Each field is `true` if that aspect of the config changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigDelta {
    pub theme_changed: bool,
    pub persistence_changed: bool,
    pub sidebar_changed: bool,
    pub font_changed: bool,
    pub scrollback_changed: bool,
    pub keybindings_changed: bool,
    pub adapters_changed: bool,
    pub ghostty_path_changed: bool,
}

impl ConfigDelta {
    /// Returns true if nothing changed.
    pub fn is_empty(&self) -> bool;

    /// Compute the delta between an old and new config.
    pub fn diff(old: &AppConfig, new: &AppConfig) -> Self;
}
```

**Files:**
- `crates/veil-core/src/config.rs` -- `ConfigDelta` and `diff` function

**Tests:**

*No changes:*
- `diff(default, default)` returns an empty delta
- `diff(config, config.clone())` returns an empty delta
- `is_empty()` returns `true` on empty delta

*Individual field changes:*
- Changing `general.theme` sets `theme_changed = true`, all others `false`
- Changing `general.persistence` sets `persistence_changed = true`
- Changing `sidebar.width` sets `sidebar_changed = true`
- Changing `sidebar.visible` sets `sidebar_changed = true`
- Changing `sidebar.default_tab` sets `sidebar_changed = true`
- Changing `terminal.font_family` sets `font_changed = true`
- Changing `terminal.font_size` sets `font_changed = true`
- Changing `terminal.font_weight` sets `font_changed = true`
- Changing `terminal.scrollback_lines` sets `scrollback_changed = true`
- Changing any `keybindings` field sets `keybindings_changed = true`
- Changing `conversations.adapters` sets `adapters_changed = true`
- Changing `ghostty.config_path` sets `ghostty_path_changed = true`

*Multiple changes:*
- Changing theme and font sets both `theme_changed` and `font_changed`
- `is_empty()` returns `false` when any field is `true`

### Unit 5: Config file watcher (hot-reload)

A background actor that monitors the config file using the `notify` crate. On file change, it re-parses the config, diffs against the current version, and invokes a callback with the new config and delta. Invalid configs are rejected (previous valid config is retained) and errors are reported via callback.

**Types:**

```rust
// crates/veil-core/src/config.rs (or config/watcher.rs)

/// Event emitted by the config watcher.
#[derive(Debug)]
pub enum ConfigEvent {
    /// Config was successfully reloaded.
    Reloaded {
        config: AppConfig,
        delta: ConfigDelta,
        warnings: Vec<ConfigWarning>,
    },
    /// Config file was modified but had errors; previous config retained.
    Error(ConfigError),
}

/// Watches the config file for changes and emits ConfigEvents.
pub struct ConfigWatcher {
    // Owns the notify::RecommendedWatcher
    // Holds the current valid config
    // Holds the path being watched
}

impl ConfigWatcher {
    /// Create a new watcher for the given config file path.
    /// `event_tx` is a channel sender for delivering config events.
    /// The watcher does NOT start until `start()` is called.
    pub fn new(
        config_path: PathBuf,
        initial_config: AppConfig,
        event_tx: tokio::sync::mpsc::Sender<ConfigEvent>,
    ) -> Result<Self, ConfigError>;

    /// Start watching for file changes.
    /// This spawns an internal task that runs until the watcher is dropped
    /// or a shutdown signal is received.
    pub fn start(&mut self, shutdown: ShutdownHandle) -> Result<(), ConfigError>;

    /// Get the currently active (valid) config.
    pub fn current_config(&self) -> &AppConfig;

    /// Manually trigger a reload (useful for testing or user-initiated reload).
    pub fn reload(&mut self) -> Result<ConfigEvent, ConfigError>;
}
```

**Files:**
- `crates/veil-core/src/config.rs` -- `ConfigWatcher`, `ConfigEvent`

**Dependencies added to veil-core `Cargo.toml`:**
- `notify = "8"` (add to workspace dependencies first, then to veil-core)

**Tests:**

*Construction:*
- `ConfigWatcher::new` with valid path and config succeeds
- `current_config()` returns the initial config

*Manual reload:*
- `reload()` after writing a valid new config returns `ConfigEvent::Reloaded` with correct delta
- `reload()` after writing invalid TOML returns `ConfigEvent::Error`
- `reload()` with no file changes returns `ConfigEvent::Reloaded` with empty delta
- After failed reload, `current_config()` still returns previous valid config

*File watcher integration (requires tempfile):*
- Write a config file, create watcher, modify file, receive `ConfigEvent::Reloaded` on channel
- Modify file with invalid TOML, receive `ConfigEvent::Error` on channel, previous config retained
- Modify file back to valid TOML after error, receive `ConfigEvent::Reloaded`
- Delete and recreate file triggers reload

*Delta correctness:*
- Changing only the theme field produces delta with only `theme_changed = true`
- Changing font and sidebar produces delta with both flags set

*Shutdown:*
- Watcher stops cleanly when shutdown signal is triggered
- Channel is closed after watcher stops

Note: File watcher tests should use `tempfile` crate for isolated temp directories and include short sleeps/retries to account for filesystem notification latency. Tests that rely on filesystem events should be gated behind `#[cfg(not(target_os = "windows"))]` if they prove flaky on Windows CI (notify's behavior varies by platform).

### Unit 6: Wire StateUpdate::ConfigReloaded with updated payload

The existing `StateUpdate::ConfigReloaded` variant in `message.rs` currently carries a `Box<SidebarConfig>`. Update it to carry the full `AppConfig` and `ConfigDelta` so the event loop can make informed decisions about what to update.

**Changes:**

```rust
// crates/veil-core/src/message.rs

// Change this:
//   ConfigReloaded(Box<SidebarConfig>),
// To:
/// Config was reloaded from disk. Contains the new full config and what changed.
ConfigReloaded {
    config: Box<AppConfig>,
    delta: ConfigDelta,
    warnings: Vec<ConfigWarning>,
},
```

Remove the `SidebarConfig` struct from `message.rs` (it is superseded by the full config model's `SidebarConfig` in `config.rs`).

**Files:**
- `crates/veil-core/src/message.rs` -- Update `StateUpdate::ConfigReloaded` variant

**Tests:**

- `StateUpdate::ConfigReloaded` can be constructed and pattern-matched with new payload shape
- Round-trip through mpsc channel preserves config and delta
- Existing tests in `message.rs` updated to use new variant shape

## Acceptance Criteria

1. `cargo build -p veil-core` succeeds with all config modules
2. `cargo test -p veil-core` passes all config tests (data model, parsing, discovery, diffing, watcher)
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `AppConfig::default()` produces documented default values for all fields
6. Empty TOML file parses successfully, producing `AppConfig::default()`
7. Partial TOML fills in defaults for unspecified fields
8. TOML parse errors include file path and line/column information
9. `ConfigDelta::diff` correctly identifies which config sections changed
10. `ConfigWatcher::reload()` re-reads and re-parses the config file
11. Invalid config during hot-reload retains the previous valid config and reports the error
12. `ConfigWatcher` stops cleanly on shutdown signal
13. No Ghostty config parsing logic exists (just the `config_path` placeholder field)
14. The `StateUpdate::ConfigReloaded` variant carries `AppConfig` + `ConfigDelta` + warnings
15. All validation warnings surface to the caller (not silently swallowed)

## Dependencies

**New crate dependencies:**

| Location | Dependency | Version | Reason |
|----------|-----------|---------|--------|
| workspace `Cargo.toml` | `notify` | `8` | Filesystem change notifications for hot-reload |
| veil-core `Cargo.toml` | `toml` | (workspace) | Move from dev-dependencies to dependencies for TOML parsing |
| veil-core `Cargo.toml` | `notify` | (workspace) | File watcher for config hot-reload |
| veil-core `Cargo.toml` | `dirs` | (workspace) | Platform-specific config directory discovery |

**Existing dependencies already available:**
- `serde` (with `derive`) -- already in veil-core dependencies
- `thiserror` -- already in veil-core dependencies
- `tracing` -- already in veil-core dependencies
- `tokio` (with `sync`) -- already in veil-core dependencies
- `tempfile` -- already in workspace dev-dependencies (for tests)

**No new tools or external software required.**

# VEI-79: Wire Config System into App Startup

## Context

The config system (parsing, validation, hot-reload, diffing) exists in `veil-core::config` but is never loaded at startup. The app uses hardcoded defaults for everything:

- `AppState::new()` hardcodes sidebar width to 250, visible to true, active tab to Workspaces (`crates/veil-core/src/state.rs:87-101`)
- `VeilApp::new()` creates `KeybindingRegistry::with_defaults()` with no config override (`crates/veil/src/main.rs:77`)
- `Channels::new(256)` is created but never used for config events (`crates/veil/src/main.rs:75`)
- No config file is read, no `ConfigWatcher` is started, and the `StateUpdate::ConfigReloaded` variant is never received or handled

This task wires the existing config infrastructure into the app lifecycle:

1. **Load config at startup** -- before window creation, call `load_or_default()` and apply the loaded config to `AppState` and `KeybindingRegistry`
2. **Start ConfigWatcher** -- watch the config file for changes, send events through `Channels`
3. **Handle ConfigReloaded in the event loop** -- receive `StateUpdate::ConfigReloaded`, update `AppState`, rebuild keybindings, trigger font re-init when needed

### What already exists

**Config module (`crates/veil-core/src/config/`):**
- `AppConfig` -- top-level config struct with `general`, `sidebar`, `terminal`, `conversations`, `keybindings`, `ghostty`, `updates` sections (`config/model.rs`)
- `load_or_default() -> (AppConfig, Option<PathBuf>)` -- discovers config file, loads and validates it, returns defaults if missing (`config/discovery.rs:72-89`)
- `load_config(path: &Path) -> Result<AppConfig, ConfigError>` -- loads from specific path (`config/discovery.rs:64-68`)
- `validate_config(config: AppConfig) -> (AppConfig, Vec<ConfigWarning>)` -- clamps out-of-range values (`config/parse.rs:63-87`)
- `ConfigDelta::diff(old: &AppConfig, new: &AppConfig) -> ConfigDelta` -- computes what changed (`config/diff.rs:45-59`)
- `ConfigWatcher::new(path, initial_config, event_tx) -> Result<Self, ConfigError>` -- creates watcher (`config/watcher.rs:48-54`)
- `ConfigWatcher::start(&mut self, shutdown: ShutdownHandle) -> Result<(), ConfigError>` -- starts background file watching (`config/watcher.rs:60-101`)
- `ConfigEvent::Reloaded { config, delta, warnings }` and `ConfigEvent::Error(ConfigError)` (`config/watcher.rs:15-28`)

**Message system (`crates/veil-core/src/message.rs`):**
- `StateUpdate::ConfigReloaded { config: Box<AppConfig>, delta: ConfigDelta, warnings: Vec<ConfigWarning> }` -- already defined (line 41-48)
- `Channels { state_tx, state_rx, command_tx }` -- already created in `VeilApp::new()` as `channels: Channels::new(256)` (main.rs:75)

**AppState (`crates/veil-core/src/state.rs`):**
- `SidebarState { visible: bool, active_tab: SidebarTab, width_px: u32 }` -- hardcoded in `AppState::new()` (lines 87-101)
- No method to apply config to state

**KeybindingRegistry (`crates/veil-core/src/keyboard.rs`):**
- `KeybindingRegistry::with_defaults()` -- hardcoded Cmd+B, Ctrl+Shift+W, etc. (lines 103-243)
- `KeybindingRegistry::bind(&mut self, input, action)` -- add/replace a binding (line 247)
- `KeybindingsConfig` has fields like `toggle_sidebar: Option<String>`, `workspace_tab: Option<String>`, etc. -- but there is no parser to convert these strings into `KeyInput` values

**Font pipeline (`crates/veil/src/font_pipeline.rs`):**
- `FontPipeline::new(config: &FontConfig) -> anyhow::Result<Self>` -- creates font from `FontConfig { path, size_pt, dpi }` (line 21)
- The font pipeline is not currently created at startup (it exists but is `#[allow(dead_code)]` in main.rs)
- When font pipeline is wired, config changes to `terminal.font_family` and `terminal.font_size` would require re-creating the `FontPipeline`

**Lifecycle (`crates/veil-core/src/lifecycle.rs`):**
- `ShutdownSignal::new()` -- already created in `VeilApp::new()` as `shutdown: ShutdownSignal::new()` (main.rs:76)
- `ShutdownSignal::handle() -> ShutdownHandle` -- creates a handle for actors

### What's missing

1. **No config loading at startup** -- `VeilApp::new()` uses `AppState::new()` with hardcoded defaults, never calls `load_or_default()`
2. **No `AppState` method to apply config** -- `AppState` has no way to receive an `AppConfig` and update its fields
3. **No keybinding string parser** -- `KeybindingsConfig` contains `Option<String>` shortcut strings like `"ctrl+shift+w"`, but there is no function to parse these into `KeyInput` values and apply them to `KeybindingRegistry`
4. **No `ConfigWatcher` started** -- the watcher infrastructure exists but is never instantiated
5. **No bridge from `ConfigEvent` to `StateUpdate`** -- when `ConfigWatcher` emits a `ConfigEvent::Reloaded`, nothing converts it to `StateUpdate::ConfigReloaded` and sends it through `Channels.state_tx`
6. **No handler for `StateUpdate::ConfigReloaded`** -- the event loop has no code that drains `state_rx` and processes config reload messages
7. **No state_rx drain in the event loop** -- `Channels.state_rx` is created but never `recv()`'d. The event loop does not poll for background actor messages at all

## Implementation Units

### Unit 1: `AppState::apply_config()` method

Add a method to `AppState` that takes an `AppConfig` reference and updates the sidebar state to match. This is the "apply" side of config loading, used both at startup and on hot-reload.

**Location:** `crates/veil-core/src/state.rs`

**Function:**

```rust
impl AppState {
    /// Apply configuration values to the current state.
    ///
    /// Updates sidebar visibility, width, and default tab from the config.
    /// Called at startup (before first frame) and on config hot-reload.
    pub fn apply_config(&mut self, config: &AppConfig) {
        self.sidebar.visible = config.sidebar.visible;
        self.sidebar.width_px = config.sidebar.width;
        self.sidebar.active_tab = match config.sidebar.default_tab {
            DefaultTab::Workspaces => SidebarTab::Workspaces,
            DefaultTab::Conversations => SidebarTab::Conversations,
        };
    }
}
```

Note: `DefaultTab` is `veil_core::config::DefaultTab`. The `SidebarTab` and `DefaultTab` enums are separate types (one in `state.rs`, one in `config/model.rs`) because they serve different purposes -- `DefaultTab` is a persistent preference, `SidebarTab` is runtime state. The mapping is straightforward but must be explicit.

**Imports needed:** `use crate::config::{AppConfig, DefaultTab};`

**Tests:**

- `apply_config` sets sidebar width from config
- `apply_config` sets sidebar visibility from config
- `apply_config` sets active tab from config default_tab
- `apply_config` with default config matches `AppState::new()` defaults (since config defaults and state defaults should agree)
- `apply_config` with non-default values correctly overrides state
- `apply_config` called twice with different configs updates state each time
- `apply_config` does not reset workspaces, conversations, or notifications

### Unit 2: Keybinding string parser

The `KeybindingsConfig` stores keybindings as `Option<String>` values like `"ctrl+shift+w"`, `"cmd+b"`, `"ctrl+n"`. These need to be parsed into `KeyInput` structs and applied to the `KeybindingRegistry`. This parser does not exist yet.

**Location:** `crates/veil-core/src/keyboard.rs`

**Functions:**

```rust
/// Parse a keybinding string like "ctrl+shift+w" or "cmd+b" into a KeyInput.
///
/// Format: modifier+modifier+key
/// Modifiers: ctrl, shift, alt, cmd/logo/super
/// Keys: single characters (a-z, 0-9, symbols) or named keys (enter, tab, escape, f1-f12, etc.)
///
/// Returns None if the string is empty or unparseable.
pub fn parse_keybinding(s: &str) -> Option<KeyInput>

/// Apply keybindings from config to the registry.
///
/// For each non-None field in `KeybindingsConfig`, parses the keybinding
/// string and rebinds the corresponding action. Unknown/unparseable strings
/// are logged as warnings and skipped.
///
/// Returns a list of warnings for keybinding strings that could not be parsed.
pub fn apply_keybindings_config(
    registry: &mut KeybindingRegistry,
    config: &KeybindingsConfig,
) -> Vec<String>
```

The `apply_keybindings_config` function maps `KeybindingsConfig` fields to `KeyAction` variants:

| Config field | KeyAction |
|---|---|
| `toggle_sidebar` | `ToggleSidebar` |
| `workspace_tab` | `SwitchToWorkspacesTab` |
| `conversations_tab` | `SwitchToConversationsTab` |
| `new_workspace` | `CreateWorkspace` |
| `close_workspace` | `CloseWorkspace` |
| `split_horizontal` | `SplitHorizontal` |
| `split_vertical` | `SplitVertical` |
| `close_pane` | `ClosePane` |
| `focus_next_pane` | `FocusNextPane` |
| `focus_previous_pane` | `FocusPreviousPane` |
| `zoom_pane` | `ZoomPane` |
| `focus_pane_left` | `FocusPaneLeft` |
| `focus_pane_right` | `FocusPaneRight` |
| `focus_pane_up` | `FocusPaneUp` |
| `focus_pane_down` | `FocusPaneDown` |

**Design notes:**

- `cmd` is an alias for `logo` (macOS users will write `cmd`, Linux/Windows users will write `super` or `logo`)
- Named keys are case-insensitive: `Enter`, `enter`, `ENTER` all work
- The parser must handle the cross-platform `Modifiers` correctly: `cmd` maps to `Modifiers { logo: true, .. }`, not `Modifiers { ctrl: true, .. }`
- `apply_keybindings_config` starts from the existing defaults (it does NOT clear the registry first), so only the explicitly set config keys are overridden

**Tests:**

*`parse_keybinding`:*
- `"ctrl+shift+w"` -> `KeyInput { key: Character('w'), modifiers: { ctrl: true, shift: true } }`
- `"cmd+b"` -> `KeyInput { key: Character('b'), modifiers: { logo: true } }`
- `"logo+b"` -> same as `cmd+b`
- `"super+b"` -> same as `cmd+b`
- `"alt+n"` -> `KeyInput { key: Character('n'), modifiers: { alt: true } }`
- `"ctrl+shift+enter"` -> `KeyInput { key: Named("Enter"), modifiers: { ctrl: true, shift: true } }`
- `"f1"` -> `KeyInput { key: Named("F1"), modifiers: default }`
- `"ctrl+f5"` -> `KeyInput { key: Named("F5"), modifiers: { ctrl: true } }`
- `""` (empty) -> `None`
- `"invalid"` with no `+` and not a known key -> `None`
- `"ctrl+"` (trailing plus) -> `None`
- `"+b"` (leading plus) -> `None`
- `"ctrl+ctrl+b"` (duplicate modifier) -> valid, `{ ctrl: true, key: 'b' }`
- `"ctrl+shift+B"` (uppercase char) -> `KeyInput { key: Character('b'), modifiers: { ctrl: true, shift: true } }` (normalized to lowercase)
- `"escape"` -> `KeyInput { key: Named("Escape"), modifiers: default }`
- `"tab"` -> `KeyInput { key: Named("Tab"), modifiers: default }`
- Named keys: `space`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`

*`apply_keybindings_config`:*
- Default `KeybindingsConfig` (all None) produces no warnings and does not change the registry
- Setting `toggle_sidebar = "ctrl+b"` overrides the existing ToggleSidebar binding
- Setting an unparseable string logs a warning and does not crash
- Setting multiple fields overrides each corresponding binding
- Unset fields (None) leave existing defaults untouched

### Unit 3: Config loading at startup

Wire `load_or_default()` into `VeilApp::new()`, apply the loaded config to `AppState` and `KeybindingRegistry`.

**Location:** `crates/veil/src/main.rs`

**Changes to `VeilApp::new()`:**

```rust
fn new() -> Self {
    // Load config from disk or use defaults
    let (config, config_path) = veil_core::config::load_or_default();

    let mut app_state = AppState::new();
    app_state.apply_config(&config);

    let mut keybindings = KeybindingRegistry::with_defaults();
    let kb_warnings = keyboard::apply_keybindings_config(
        &mut keybindings,
        &config.keybindings,
    );
    for w in &kb_warnings {
        tracing::warn!("keybinding config warning: {w}");
    }

    if let Some(ref path) = config_path {
        tracing::info!("loaded config from {}", path.display());
    } else {
        tracing::info!("no config file found, using defaults");
    }

    Self {
        window: None,
        renderer: None,
        app_state,
        channels: Channels::new(256),
        shutdown: ShutdownSignal::new(),
        keybindings,
        focus: FocusManager::new(),
        current_modifiers: keyboard::Modifiers::default(),
        window_size: (1280, 800),
        pty_manager: None,
        config_path,       // new field
        config_watcher: None, // new field, started in resumed()
    }
}
```

**New fields on `VeilApp`:**

```rust
/// Path to the config file, if one was found.
config_path: Option<std::path::PathBuf>,
/// Config file watcher for hot-reload. Started after window creation.
config_watcher: Option<veil_core::config::ConfigWatcher>,
```

**Tests:**

This unit's correctness is validated through integration-style tests in `bootstrap.rs` and by the unit tests in Units 1 and 2. Direct testing of `VeilApp::new()` requires a window/event loop and is not feasible in unit tests.

- Add a test to `bootstrap.rs`: `init_with_config_applies_sidebar_settings` -- creates an `AppConfig` with non-default sidebar width, applies it to `AppState`, verifies the width is correct before bootstrapping the workspace
- Add a test: `init_with_config_does_not_crash_with_missing_file` -- `load_or_default()` returns defaults when no file exists, state is valid

### Unit 4: Start ConfigWatcher in `resumed()`

After the window is created, start the `ConfigWatcher` if a config file path was discovered. The watcher sends `ConfigEvent`s through a dedicated channel; a bridge task converts them to `StateUpdate::ConfigReloaded` and forwards them to `Channels.state_tx`.

**Location:** `crates/veil/src/main.rs`

**Changes to `resumed()`:**

After the existing window/renderer/PTY setup, add:

```rust
// Start config file watcher if a config path is available.
if let Some(ref config_path) = self.config_path {
    let (config_event_tx, mut config_event_rx) =
        tokio::sync::mpsc::channel::<veil_core::config::ConfigEvent>(16);

    let initial_config = {
        // Reconstruct current config from state (or store it on VeilApp)
        // Simplest approach: store the loaded AppConfig on VeilApp
    };

    match veil_core::config::ConfigWatcher::new(
        config_path.clone(),
        initial_config,
        config_event_tx,
    ) {
        Ok(mut watcher) => {
            if let Err(e) = watcher.start(self.shutdown.handle()) {
                tracing::warn!("failed to start config watcher: {e}");
            } else {
                tracing::info!("config watcher started for {}", config_path.display());
                self.config_watcher = Some(watcher);
            }
        }
        Err(e) => {
            tracing::warn!("failed to create config watcher: {e}");
        }
    }

    // Bridge: forward ConfigEvents to state_tx as StateUpdate::ConfigReloaded.
    let state_tx = self.channels.state_tx.clone();
    std::thread::spawn(move || {
        while let Some(event) = config_event_rx.blocking_recv() {
            let update = match event {
                veil_core::config::ConfigEvent::Reloaded {
                    config, delta, warnings,
                } => StateUpdate::ConfigReloaded { config, delta, warnings },
                veil_core::config::ConfigEvent::Error(e) => {
                    tracing::warn!("config reload error: {e}");
                    continue;
                }
            };
            if state_tx.blocking_send(update).is_err() {
                break; // receiver dropped
            }
        }
    });
}
```

**Design note:** The `ConfigWatcher` uses its own `tokio::sync::mpsc` channel internally. Since `VeilApp` runs on the winit event loop (not a tokio runtime), we use `blocking_recv()` in a separate thread to bridge the async channel to the sync world. The bridge thread converts `ConfigEvent` to `StateUpdate` and forwards via `state_tx.blocking_send()`.

**Alternative considered:** Store the `AppConfig` on `VeilApp` so we don't need to reconstruct it. This is cleaner -- add an `app_config: AppConfig` field to `VeilApp`.

**New field on `VeilApp`:**

```rust
/// The current application config (loaded at startup, updated on reload).
app_config: veil_core::config::AppConfig,
```

This field is set in `new()` from the loaded config and updated when `StateUpdate::ConfigReloaded` is handled (Unit 5).

**Tests:**

- The `ConfigWatcher` is already thoroughly tested in `veil-core` (`config/mod.rs` watcher_tests)
- The bridge logic is minimal (map + send) -- tested implicitly through integration
- Add a test in `bootstrap.rs`: `config_watcher_does_not_crash_when_no_config_path` -- verifies `config_watcher` stays `None` when `config_path` is `None`

### Unit 5: Handle `StateUpdate::ConfigReloaded` in the event loop

Drain `state_rx` during `RedrawRequested` (or at the start of each frame) and process config reload messages by updating `AppState`, `KeybindingRegistry`, and flagging font re-init when needed.

**Location:** `crates/veil/src/main.rs`

**New method on `VeilApp`:**

```rust
impl VeilApp {
    /// Drain pending state updates from the channel and apply them.
    fn drain_state_updates(&mut self) {
        while let Ok(update) = self.channels.state_rx.try_recv() {
            match update {
                StateUpdate::ConfigReloaded { config, delta, warnings } => {
                    self.handle_config_reloaded(*config, delta, warnings);
                }
                // Future: handle other StateUpdate variants (PtyOutput, etc.)
                _ => {}
            }
        }
    }

    fn handle_config_reloaded(
        &mut self,
        config: AppConfig,
        delta: ConfigDelta,
        warnings: Vec<ConfigWarning>,
    ) {
        for w in &warnings {
            tracing::warn!("config validation: {}: {}", w.field, w.message);
        }

        if delta.is_empty() {
            tracing::debug!("config reloaded with no changes");
            return;
        }

        // Apply sidebar changes
        if delta.sidebar_changed {
            self.app_state.apply_config(&config);
            tracing::info!("sidebar config updated");
        }

        // Apply keybinding changes
        if delta.keybindings_changed {
            self.keybindings = KeybindingRegistry::with_defaults();
            let kb_warnings = keyboard::apply_keybindings_config(
                &mut self.keybindings,
                &config.keybindings,
            );
            for w in &kb_warnings {
                tracing::warn!("keybinding config warning: {w}");
            }
            tracing::info!("keybindings reloaded");
        }

        // Flag font re-init needed (actual font pipeline not yet wired)
        if delta.font_changed {
            tracing::info!(
                "font config changed (family={:?}, size={}, weight={:?}) -- \
                 font re-init will apply when font pipeline is wired",
                config.terminal.font_family,
                config.terminal.font_size,
                config.terminal.font_weight,
            );
            // TODO(VEI-future): recreate FontPipeline with new settings
        }

        // Log theme change (actual theme application depends on egui/renderer theming)
        if delta.theme_changed {
            tracing::info!("theme changed to {:?}", config.general.theme);
            // TODO(VEI-future): apply theme to egui context and terminal colors
        }

        // Store updated config
        self.app_config = config;

        // Request redraw to reflect changes
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}
```

**Call site:** Add `self.drain_state_updates()` at the start of `handle_redraw()`, before `build_frame_geometry()`:

```rust
fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
    self.drain_state_updates();

    let frame_geometry = build_frame_geometry(
        &self.app_state,
        &self.focus,
        self.window_size.0,
        self.window_size.1,
    );
    // ... rest of existing code
}
```

**Tests:**

The event loop handler is not directly testable without a window, but the config application logic is tested via the `AppState::apply_config()` tests (Unit 1) and the keybinding parser tests (Unit 2). Additional tests:

- `handle_config_reloaded` with empty delta is a no-op (tested via logging/tracing assertions)
- `handle_config_reloaded` with `sidebar_changed` updates sidebar state
- `handle_config_reloaded` with `keybindings_changed` rebuilds registry

Since `handle_config_reloaded` is a method on `VeilApp` which requires a window, extract the core logic into testable free functions:

```rust
// In crates/veil/src/main.rs or a new config_apply.rs module

/// Apply a config reload to app state and keybindings.
/// Returns true if a redraw is needed.
pub fn apply_config_reload(
    config: &AppConfig,
    delta: &ConfigDelta,
    app_state: &mut AppState,
    keybindings: &mut KeybindingRegistry,
) -> bool {
    let mut needs_redraw = false;

    if delta.sidebar_changed {
        app_state.apply_config(config);
        needs_redraw = true;
    }

    if delta.keybindings_changed {
        *keybindings = KeybindingRegistry::with_defaults();
        let _warnings = keyboard::apply_keybindings_config(keybindings, &config.keybindings);
        needs_redraw = true;
    }

    if delta.font_changed || delta.theme_changed {
        needs_redraw = true;
    }

    needs_redraw
}
```

Tests for `apply_config_reload`:
- Empty delta returns false (no redraw needed)
- sidebar_changed applies config to app_state, returns true
- keybindings_changed rebuilds registry with new bindings, returns true
- font_changed returns true (flag for re-init)
- theme_changed returns true
- Multiple changes all applied, returns true
- Non-matching delta fields do not modify state

## Test Strategy Summary

| Unit | What | Test type | Location |
|------|------|-----------|----------|
| 1: `apply_config` | Sidebar state from config | Unit tests | `crates/veil-core/src/state.rs` |
| 2: `parse_keybinding` | String -> KeyInput parsing | Unit tests | `crates/veil-core/src/keyboard.rs` |
| 2: `apply_keybindings_config` | Config -> registry binding | Unit tests | `crates/veil-core/src/keyboard.rs` |
| 3: Startup loading | Config loaded before window | Integration (compile check + bootstrap tests) | `crates/veil/src/bootstrap.rs` |
| 4: ConfigWatcher start | Watcher created and started | Covered by existing `veil-core` watcher tests | `crates/veil-core/src/config/mod.rs` |
| 5: `apply_config_reload` | Delta-driven state update | Unit tests | `crates/veil/src/main.rs` or new module |
| Existing | ConfigWatcher hot-reload | Async tests with tempfile | `crates/veil-core/src/config/mod.rs` |
| Existing | ConfigDelta diffing | Unit tests | `crates/veil-core/src/config/mod.rs` |
| Existing | StateUpdate::ConfigReloaded channel | Async channel round-trip | `crates/veil-core/src/config/mod.rs` |

## Acceptance Criteria

- [ ] Config file is read at startup via `load_or_default()` (or defaults used if missing)
- [ ] `AppState` sidebar width, visibility, and default tab match the loaded config at startup
- [ ] `KeybindingRegistry` reflects custom keybindings from config at startup
- [ ] Invalid config file produces a warning log, not a crash; defaults are used
- [ ] `ConfigWatcher` is started when a config file path exists
- [ ] `ConfigWatcher` is not started (no error) when no config file path exists
- [ ] Editing config.toml triggers hot-reload: changes apply without restart
- [ ] Hot-reloaded sidebar changes (width, visibility) take effect immediately
- [ ] Hot-reloaded keybinding changes take effect immediately
- [ ] Font changes are logged (actual font re-init deferred to font pipeline wiring)
- [ ] Theme changes are logged (actual theme application deferred to theme wiring)
- [ ] Invalid config during hot-reload retains previous valid config
- [ ] Validation warnings from hot-reload are logged
- [ ] `state_rx` is drained each frame (no unbounded queue growth)
- [ ] All existing tests pass (`cargo test`)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Dependencies

**No new crate dependencies required.** All necessary crates are already in the workspace:

| Crate | Already in | Used for |
|-------|-----------|----------|
| `veil-core::config` | `veil-core` | `AppConfig`, `load_or_default`, `ConfigWatcher`, `ConfigDelta`, `ConfigWarning` |
| `toml` | `veil-core` dependencies | TOML parsing (already used by config module) |
| `notify` | `veil-core` dependencies | File watching (already used by ConfigWatcher) |
| `dirs` | `veil-core` dependencies | Config path discovery (already used by discovery module) |
| `tracing` | all crates | Logging |
| `tokio::sync` | `veil-core` dependencies | Channels (already used) |
| `tempfile` | workspace dev-dependencies | Tests (already available) |

**Files to modify:**

| File | Changes |
|------|---------|
| `crates/veil-core/src/state.rs` | Add `apply_config()` method to `AppState`, add import for config types |
| `crates/veil-core/src/keyboard.rs` | Add `parse_keybinding()` and `apply_keybindings_config()` functions |
| `crates/veil/src/main.rs` | Add config loading in `new()`, add `config_path`/`config_watcher`/`app_config` fields to `VeilApp`, start watcher in `resumed()`, add bridge thread, add `drain_state_updates()` and `handle_config_reloaded()`, call drain in `handle_redraw()` |
| `crates/veil/src/bootstrap.rs` | Add integration tests for config-aware startup |

**No new files needed** (unless the implementer extracts `apply_config_reload` into a separate module for testability, which is recommended but optional).

## Implementation Order

Units 1 and 2 are independent and can be implemented in parallel. Unit 3 depends on both (it calls `apply_config` and `apply_keybindings_config`). Unit 4 depends on Unit 3 (needs `app_config` field). Unit 5 depends on Units 1, 2, and 4.

```
Unit 1 (apply_config)  ──┐
                          ├──> Unit 3 (startup loading) ──> Unit 4 (watcher) ──> Unit 5 (reload handler)
Unit 2 (keybinding parser) ┘
```

Recommended commit sequence:
1. `test(VEI-79): RED for config-to-state application and keybinding parsing` -- failing tests for Units 1 and 2
2. `feat(VEI-79): implement apply_config and keybinding string parser` -- make tests green for Units 1 and 2
3. `feat(VEI-79): wire config loading at startup and hot-reload handling` -- Units 3, 4, and 5
4. `refactor(VEI-79): extract apply_config_reload for testability` -- if needed

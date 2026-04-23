# VEI-75: Wire Keyboard Input to PTY and Key Action Dispatch

## Context

The app creates a window, spawns a PTY, and renders frame geometry (VEI-74), but keyboard events are completely ignored. The `_keybindings` field on `VeilApp` is underscore-prefixed (unused), and the `window_event` handler's `_ => {}` catch-all discards all `KeyboardInput` and `ModifiersChanged` events. The user cannot type into the terminal or use any keyboard shortcuts.

This task connects winit's keyboard events to two destinations:

1. **Key actions** -- Global shortcuts (Cmd+D, Cmd+B, Cmd+W, etc.) matched via `KeybindingRegistry::lookup()` dispatch to `AppState` mutation methods, with side effects like PTY spawn/close.
2. **PTY input** -- Unmatched key events on a focused terminal surface are encoded as bytes and written to that surface's PTY via `PtyManager::write()`.

The existing infrastructure is ready. `veil_core::focus::route_key_event()` already implements the routing decision (global action vs. forward-to-surface vs. forward-to-sidebar vs. unhandled). `PtyManager::write(surface_id, data)` already sends bytes to a PTY. What's missing is the translation layer between winit's event types and the domain types, plus the dispatch logic in `window_event`.

### What exists

- **`KeybindingRegistry`** (`veil_core::keyboard`) -- `with_defaults()` populates all default shortcuts (Cmd+D, Cmd+B, Cmd+W, Cmd+N, etc.). `lookup(&KeyInput) -> Option<&KeyAction>` resolves a key press.
- **`KeyInput`** / **`Key`** / **`Modifiers`** (`veil_core::keyboard`) -- Domain types for key events. `Key::Character(char)` and `Key::Named(String)`.
- **`KeyAction`** (`veil_core::keyboard`) -- Enum with all actions: `SplitHorizontal`, `SplitVertical`, `ClosePane`, `FocusNextPane`, `FocusPreviousPane`, `ZoomPane`, `ToggleSidebar`, `CreateWorkspace`, `SwitchWorkspace(u8)`, `CloseWorkspace`, `SwitchToWorkspacesTab`, `SwitchToConversationsTab`, `FocusPaneLeft/Right/Up/Down`, `RenameWorkspace`, `FocusSidebar`, `FocusTerminal`.
- **`FocusManager`** (`veil_core::focus`) -- Tracks `FocusTarget::Surface(SurfaceId)` or `FocusTarget::Sidebar`. `focused_surface() -> Option<SurfaceId>`, `focus_surface(id)`, `focus_sidebar()`, `clear()`.
- **`route_key_event(input, registry, focus) -> KeyRoute`** (`veil_core::focus`) -- Checks bindings first, then routes to focus target. Returns `KeyRoute::Action(action)`, `KeyRoute::ForwardToSurface(id)`, `KeyRoute::ForwardToSidebar`, or `KeyRoute::Unhandled`.
- **`AppState`** (`veil_core::state`) -- Has `split_pane(ws_id, pane_id, direction) -> (PaneId, SurfaceId)`, `close_pane(ws_id, pane_id) -> Option<SurfaceId>`, `toggle_sidebar()`, `toggle_zoom(ws_id, pane_id)`, `create_workspace(name, cwd) -> WorkspaceId`, `set_active_workspace(ws_id)`, `close_workspace(ws_id) -> Vec<SurfaceId>`, `set_sidebar_tab(tab)`, `active_workspace()`, `active_workspace_mut()`.
- **`PtyManager`** (`veil_pty::manager`) -- Has `write(surface_id, data)`, `spawn(surface_id, config)`, `close(surface_id)`, `shutdown_all()`. Keyed by `SurfaceId`.
- **`VeilApp`** (`crates/veil/src/main.rs`) -- Owns `app_state: AppState`, `focus: FocusManager`, `_keybindings: KeybindingRegistry`, `pty_manager: Option<PtyManager>`, `channels: Channels`, `shutdown: ShutdownSignal`, `window: Option<Arc<Window>>`, `window_size: (u32, u32)`.
- **`Workspace`** (`veil_core::workspace`) -- Has `pane_ids() -> Vec<PaneId>`, `layout: PaneNode` with `surface_ids()`, `pane_id_for_surface()` (does NOT exist yet -- see Unit 3), `working_directory: PathBuf`.
- **`veil_core::navigation::find_pane_in_direction`** -- spatial navigation using computed pane rects.
- **`veil_core::layout::compute_layout`** -- computes `Vec<PaneLayout>` from a `PaneNode` and available rect.

### What's missing

1. **winit-to-domain key translation** -- Converting winit 0.30's `KeyEvent` (with `logical_key: Key<SmolStr>`, `state: ElementState`) and `ModifiersState` into the domain `KeyInput` type.
2. **PTY byte encoding** -- Converting key events into the byte sequences terminals expect (UTF-8 for printable chars, control codes for Ctrl+letter, ANSI escape sequences for arrow keys and special keys).
3. **Reverse lookup `SurfaceId -> PaneId`** -- `AppState::split_pane`, `close_pane`, and `toggle_zoom` all take `PaneId`, but `FocusManager` tracks `SurfaceId`. No reverse mapping exists on `PaneNode`.
4. **Action dispatcher** -- A function that takes a `KeyAction` and performs the action on `AppState`/`FocusManager`, returning side effects (PTY spawn, PTY close, redraw request).
5. **Event loop wiring** -- `ModifiersChanged` and `KeyboardInput` match arms in `window_event`, connecting translation -> routing -> dispatch/PTY-write.
6. **Bracket keybinding fix** -- `with_defaults()` binds `Key::Named("[")` / `Key::Named("]")` for `FocusPreviousPane` / `FocusNextPane`, but winit reports `[` and `]` as `Key::Character`, not named keys. Needs correction.

## Implementation Units

### Unit 1: winit key event translation (`key_translation` module)

**Location:** `crates/veil/src/key_translation.rs` (new module)

**What it does:**

Pure functions that convert winit 0.30's keyboard types into the domain `KeyInput` type. No side effects, no PTY interaction.

winit 0.30.13 provides:
- `KeyEvent { logical_key: Key<SmolStr>, physical_key: PhysicalKey, state: ElementState, text: Option<SmolStr>, repeat: bool, .. }`
- `Key::Character(SmolStr)` -- the logical character
- `Key::Named(NamedKey)` -- Enter, Escape, Tab, ArrowUp, etc.
- `Modifiers` (from `ModifiersChanged`) with `state() -> ModifiersState` which has `super_key()`, `control_key()`, `shift_key()`, `alt_key()`

**Functions:**

```rust
/// Convert winit ModifiersState into domain Modifiers.
pub fn translate_modifiers(state: &winit::keyboard::ModifiersState) -> keyboard::Modifiers

/// Convert a winit KeyEvent + tracked modifiers into a domain KeyInput.
/// Returns None for key releases, modifier-only presses, or untranslatable keys.
pub fn translate_key_event(
    event: &winit::event::KeyEvent,
    modifiers: &keyboard::Modifiers,
) -> Option<KeyInput>
```

Translation rules:
- Only process `ElementState::Pressed` (releases return `None`).
- `winit::keyboard::Key::Character(text)` -> take the first char, lowercase it when a modifier (logo/ctrl) is held (so Cmd+Shift+D with char `'D'` normalizes to `Key::Character('d')` with `shift: true`). This ensures keybinding lookup works regardless of shift state.
- `winit::keyboard::Key::Named(NamedKey::Enter)` -> `Key::Named("Enter")`, and so on for all relevant named keys. Map `NamedKey::BracketLeft`/`BracketRight` if they exist, otherwise brackets come through as `Key::Character`.
- Modifier-only presses (just Shift, just Cmd) -> `None`.
- The domain `Modifiers` are passed in pre-translated (from the tracked state updated by `ModifiersChanged`), not re-derived from the event.

**Tests:**

1. **Character 'a', no modifiers:** Returns `KeyInput { key: Key::Character('a'), modifiers: default }`.
2. **Character 'd', logo modifier:** Returns `KeyInput { key: Key::Character('d'), modifiers: { logo: true } }`.
3. **Character 'D' (uppercase), logo+shift:** Normalizes to `Key::Character('d')` with `{ logo: true, shift: true }`.
4. **Named key Enter:** Returns `KeyInput { key: Key::Named("Enter"), modifiers: default }`.
5. **Named key ArrowUp:** Returns `Key::Named("ArrowUp")`.
6. **Named key Tab:** Returns `Key::Named("Tab")`.
7. **Named key Escape:** Returns `Key::Named("Escape")`.
8. **Key release:** Returns `None`.
9. **Modifier-only (e.g., `Key::Named(NamedKey::Super)`):** Returns `None`.
10. **Multi-char SmolStr:** Takes first char.
11. **Bracket character `[` with logo:** Returns `Key::Character('[')` with `{ logo: true }`.

### Unit 2: PTY input encoding (`key_encoding` module)

**Location:** `crates/veil/src/key_encoding.rs` (new module)

**What it does:**

Converts a winit key event into the byte sequence to write to a PTY. Called only for keys routed as `ForwardToSurface` (after keybinding lookup failed). Pure function, no side effects.

```rust
/// Encode a key event as bytes for PTY input.
/// Returns None if the key produces no bytes (modifier-only, unrecognized key).
pub fn encode_key_for_pty(
    event: &winit::event::KeyEvent,
    modifiers: &keyboard::Modifiers,
) -> Option<Vec<u8>>
```

Encoding rules:
- **Printable text:** Use `event.text` (winit's pre-composed text) when available, encoded as UTF-8. This handles IME output, dead keys, and multi-byte characters. Falls back to `logical_key` character if `text` is `None`.
- **Ctrl+letter (a-z):** Encode as control character: `(char as u8) - 0x60`. Ctrl+A=0x01, Ctrl+C=0x03, Ctrl+D=0x04, ..., Ctrl+Z=0x1A. Important: do NOT generate control codes for Logo/Cmd+letter -- Cmd is not Ctrl.
- **Named keys:**
  - Enter -> `\r` (0x0D)
  - Tab -> `\t` (0x09)
  - Escape -> `\x1b` (0x1B)
  - Backspace -> `\x7f` (0x7F)
  - Space -> `\x20` (0x20)
  - ArrowUp -> `\x1b[A`, ArrowDown -> `\x1b[B`, ArrowRight -> `\x1b[C`, ArrowLeft -> `\x1b[D`
  - Home -> `\x1b[H`, End -> `\x1b[F`
  - Delete -> `\x1b[3~`
  - PageUp -> `\x1b[5~`, PageDown -> `\x1b[6~`
  - Insert -> `\x1b[2~`
  - F1 -> `\x1bOP`, F2 -> `\x1bOQ`, F3 -> `\x1bOR`, F4 -> `\x1bOS`
  - F5-F12 -> `\x1b[15~` through `\x1b[24~` (standard xterm sequences)
- **Other named keys / modifier-only:** Return `None`.

Note: This is a minimal encoding sufficient to make the terminal interactive. Full VT input encoding (Kitty keyboard protocol, mouse events, application cursor mode) will come when libghosty input encoding is integrated.

**Tests:**

1. **'a' -> `[0x61]`**
2. **'e' with accent (multi-byte UTF-8) -> correct UTF-8 bytes**
3. **Ctrl+C -> `[0x03]`**
4. **Ctrl+D -> `[0x04]`**
5. **Ctrl+A -> `[0x01]`**
6. **Ctrl+Z -> `[0x1A]`**
7. **Logo+D (Cmd+D with no keybinding match) -> `[0x64]` (just the character 'd', NOT a control code)**
8. **Enter -> `[0x0D]`**
9. **Tab -> `[0x09]`**
10. **Escape -> `[0x1B]`**
11. **Backspace -> `[0x7F]`**
12. **ArrowUp -> `[0x1B, 0x5B, 0x41]`**
13. **ArrowDown -> `[0x1B, 0x5B, 0x42]`**
14. **ArrowRight -> `[0x1B, 0x5B, 0x43]`**
15. **ArrowLeft -> `[0x1B, 0x5B, 0x44]`**
16. **Delete -> `[0x1B, 0x5B, 0x33, 0x7E]`**
17. **Home -> `[0x1B, 0x5B, 0x48]`**
18. **End -> `[0x1B, 0x5B, 0x46]`**
19. **F1 -> `[0x1B, 0x4F, 0x50]`**
20. **Shift alone -> `None`**
21. **Logo alone -> `None`**
22. **Space -> `[0x20]`**

### Unit 3: Reverse lookup `PaneNode::pane_id_for_surface`

**Location:** `crates/veil-core/src/workspace.rs`

**What it does:**

Adds a method to `PaneNode` and `Workspace` that finds a `PaneId` given a `SurfaceId`. This is the inverse of `find_pane` (which finds by `PaneId`). Needed because `AppState::split_pane`, `close_pane`, and `toggle_zoom` all require `PaneId`, but `FocusManager` tracks `SurfaceId`.

```rust
impl PaneNode {
    /// Find the pane ID associated with a surface ID.
    pub fn pane_id_for_surface(&self, target: SurfaceId) -> Option<PaneId> {
        match self {
            PaneNode::Leaf { pane_id, surface_id } => {
                if *surface_id == target { Some(*pane_id) } else { None }
            }
            PaneNode::Split { first, second, .. } => {
                first.pane_id_for_surface(target)
                    .or_else(|| second.pane_id_for_surface(target))
            }
        }
    }
}
```

Also add a convenience method on `Workspace`:

```rust
impl Workspace {
    /// Find the pane ID for a given surface ID in this workspace's layout.
    pub fn pane_id_for_surface(&self, surface_id: SurfaceId) -> Option<PaneId> {
        self.layout.pane_id_for_surface(surface_id)
    }
}
```

**Tests:**

1. **Single-pane workspace:** `pane_id_for_surface(root_surface)` returns `Some(root_pane)`.
2. **After split:** `pane_id_for_surface(new_surface)` returns `Some(new_pane)`.
3. **Nonexistent surface:** `pane_id_for_surface(SurfaceId::new(999))` returns `None`.
4. **Deep tree (3+ levels):** After multiple splits, each surface maps to its correct pane.
5. **After close:** Closed surface's ID returns `None`.

### Unit 4: Key action dispatch (`action_dispatch` module)

**Location:** `crates/veil/src/action_dispatch.rs` (new module)

**What it does:**

Takes a `KeyAction` and the current application state, performs the action, and returns a list of side effects for the event loop to execute. Operates on `AppState` and `FocusManager` directly. Does NOT touch `PtyManager` or `Window` -- those are handled by the event loop when it processes the returned effects.

**Types:**

```rust
/// Side effects produced by dispatching a key action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionEffect {
    /// Spawn a new PTY for the given surface.
    SpawnPty { surface_id: SurfaceId, working_directory: PathBuf },
    /// Close the PTY for the given surface.
    ClosePty { surface_id: SurfaceId },
    /// Request a window redraw (layout/visibility changed).
    Redraw,
    /// No effect.
    None,
}
```

**Functions:**

```rust
/// Dispatch a key action against the current state.
/// Returns a list of effects for the event loop to execute.
pub fn dispatch_action(
    action: &KeyAction,
    app_state: &mut AppState,
    focus: &mut FocusManager,
    window_width: u32,
    window_height: u32,
) -> Vec<ActionEffect>
```

**Action mapping:**

| KeyAction | Behavior |
|-----------|----------|
| `SplitHorizontal` | Get active workspace + focused surface -> pane ID (via `pane_id_for_surface`). Call `app_state.split_pane(ws_id, pane_id, Horizontal)`. On success, focus the new surface. Return `[SpawnPty { new_surface, cwd }, Redraw]`. |
| `SplitVertical` | Same as above with `Vertical` direction. |
| `ClosePane` | Get focused pane. If this is the last pane in the only workspace, no-op. If last pane but other workspaces exist, close the workspace. Otherwise call `app_state.close_pane(ws_id, pane_id)`. Move focus to a remaining surface in the workspace. Return `[ClosePty { surface_id }, Redraw]`. |
| `FocusNextPane` | Get surface IDs from active workspace layout. Find current index, advance circularly, update focus. Return `[Redraw]`. |
| `FocusPreviousPane` | Same, going backward. |
| `FocusPaneLeft/Right/Up/Down` | Compute layout rects. Use `find_pane_in_direction` to find neighbor. Update focus to neighbor's surface. Return `[Redraw]` or `[]` if at edge. |
| `ToggleSidebar` | `app_state.toggle_sidebar()`. Return `[Redraw]`. |
| `ZoomPane` | Get focused pane. `app_state.toggle_zoom(ws_id, pane_id)`. Return `[Redraw]`. |
| `CreateWorkspace` | Generate name. Use active workspace cwd (or fallback). `app_state.create_workspace(name, cwd)`. Activate new workspace. Focus root surface. Return `[SpawnPty { root_surface, cwd }, Redraw]`. |
| `CloseWorkspace` | Get active workspace. Collect all surface IDs. `app_state.close_workspace(ws_id)`. Focus next workspace's root surface (if any). Return `[ClosePty for each surface, Redraw]`. |
| `SwitchWorkspace(n)` | Index into `app_state.workspaces` at position `n-1`. If exists, `app_state.set_active_workspace(id)`. Focus first surface. Return `[Redraw]`. |
| `SwitchToWorkspacesTab` | `app_state.set_sidebar_tab(SidebarTab::Workspaces)`. Return `[Redraw]`. |
| `SwitchToConversationsTab` | `app_state.set_sidebar_tab(SidebarTab::Conversations)`. Return `[Redraw]`. |
| `FocusSidebar` | `focus.focus_sidebar()`. Return `[Redraw]`. |
| `FocusTerminal` | Focus first surface of active workspace. Return `[Redraw]`. |
| `RenameWorkspace` | No-op (needs text input UI). Return `[None]`. |

Key decisions:
- **Focus after split:** Focus moves to the NEW pane. Matches tmux and iTerm behavior.
- **Focus after close:** Focus moves to the next sibling surface in the layout. If closing the last in the list, wraps to the first.
- **Workspace name for `CreateWorkspace`:** `"workspace-N"` where N is the current workspace count + 1.
- **Working directory inheritance:** New panes/workspaces inherit `working_directory` from the active workspace.
- **ClosePane on last pane:** If it's the only workspace with one pane, do nothing (user has nowhere to go). If other workspaces exist, close the entire workspace.

**Tests:**

1. **SplitHorizontal: creates pane and returns SpawnPty.** Single-pane workspace, dispatch `SplitHorizontal`. Assert 2 panes, effects contain `SpawnPty` with new surface ID and `Redraw`.
2. **SplitVertical: creates pane.** Same as above, verify `SplitDirection::Vertical`.
3. **Split with no focus: returns empty.** No focused surface, assert effects are empty.
4. **Split with no active workspace: returns empty.** Fresh AppState, assert effects are empty.
5. **ClosePane: removes pane and returns ClosePty.** Two-pane workspace, close one. Assert 1 pane, effects contain `ClosePty` with closed surface and `Redraw`.
6. **ClosePane moves focus.** After close, `focus.focused_surface()` returns the remaining pane's surface.
7. **ClosePane on last pane of only workspace: no-op.** Assert no change, effects are empty.
8. **ClosePane on last pane with other workspaces: closes workspace.** Two workspaces, close last pane of active. Assert workspace removed.
9. **FocusNextPane cycles forward.** Three surfaces, focused on first. After dispatch, focused on second.
10. **FocusNextPane wraps from last to first.** Focused on last surface, wraps to first.
11. **FocusPreviousPane wraps from first to last.** Focused on first, wraps to last.
12. **ToggleSidebar flips visibility.** Assert `sidebar.visible` changes.
13. **ZoomPane toggles zoom.** Assert `zoomed_pane` is set/cleared.
14. **ZoomPane with no focus: no-op.** Assert effects empty.
15. **CreateWorkspace: adds workspace and returns SpawnPty.** Assert workspace count increases, new workspace is active, effects contain `SpawnPty` and `Redraw`.
16. **SwitchWorkspace(2) with 3 workspaces: switches.** Assert active workspace is the second.
17. **SwitchWorkspace(9) with 2 workspaces: no-op.** Assert no change.
18. **CloseWorkspace: removes workspace and returns ClosePty for each surface.** Assert workspace count decreases, all surfaces get ClosePty.
19. **SwitchToWorkspacesTab: changes tab.** Assert `sidebar.active_tab` is `Workspaces`.
20. **SwitchToConversationsTab: changes tab.** Assert `sidebar.active_tab` is `Conversations`.
21. **FocusSidebar: changes focus.** Assert `focus.current()` is `FocusTarget::Sidebar`.
22. **FocusTerminal: returns to surface.** After `FocusSidebar`, dispatch `FocusTerminal`, assert surface focused.
23. **All actions with no active workspace: return empty effects.** Exhaustive check that no action panics on empty state.

### Unit 5: Fix bracket keybinding defaults

**Location:** `crates/veil-core/src/keyboard.rs`

**What it does:**

The current `with_defaults()` binds `Key::Named("[")` and `Key::Named("]")` for `FocusPreviousPane` / `FocusNextPane`. But winit reports `[` and `]` as `Key::Character('[')` / `Key::Character(']')`, not as named keys. This means the default bindings can never match actual key events from winit.

Change the bindings to use `Key::Character`:

```rust
// Before (broken):
registry.bind(
    KeyInput { key: Key::Named("[".to_string()), modifiers: Modifiers { logo: true, ..Default::default() } },
    KeyAction::FocusPreviousPane,
);

// After (correct):
registry.bind(
    KeyInput { key: Key::Character('['), modifiers: Modifiers { logo: true, ..Default::default() } },
    KeyAction::FocusPreviousPane,
);
```

Same fix for `]` -> `FocusNextPane`.

Update existing tests that use `logo_named("[")` / `logo_named("]")` to use `logo_key('[')` / `logo_key(']')` (the `logo_key` helper already exists and takes a `char`).

**Tests:**

1. **`defaults_include_focus_previous_pane`:** Change to `logo_key('[')`, assert lookup returns `FocusPreviousPane`.
2. **`defaults_include_focus_next_pane`:** Change to `logo_key(']')`, assert lookup returns `FocusNextPane`.
3. **Old named key no longer matches:** `logo_named("[")` returns `None` (removed, so won't match).

### Unit 6: Event loop wiring in `main.rs`

**Location:** `crates/veil/src/main.rs`

**What it does:**

Adds `ModifiersChanged` and `KeyboardInput` handling to `window_event`, connecting all the pieces.

**Changes to `VeilApp` struct:**

```rust
struct VeilApp {
    // ... existing fields ...
    keybindings: KeybindingRegistry,  // renamed from _keybindings
    current_modifiers: keyboard::Modifiers,  // new: tracked modifier state
}
```

**New module declarations:**

```rust
mod action_dispatch;
mod key_encoding;
mod key_translation;
```

**New match arms in `window_event`:**

```rust
WindowEvent::ModifiersChanged(new_modifiers) => {
    self.current_modifiers = key_translation::translate_modifiers(&new_modifiers.state());
}

WindowEvent::KeyboardInput { event, .. } => {
    if event.state != ElementState::Pressed {
        return;
    }
    if let Some(key_input) = key_translation::translate_key_event(&event, &self.current_modifiers) {
        let route = route_key_event(&key_input, &self.keybindings, &self.focus);
        match route {
            KeyRoute::Action(action) => {
                let effects = action_dispatch::dispatch_action(
                    &action,
                    &mut self.app_state,
                    &mut self.focus,
                    self.window_size.0,
                    self.window_size.1,
                );
                for effect in effects {
                    self.execute_effect(effect);
                }
            }
            KeyRoute::ForwardToSurface(surface_id) => {
                if let Some(bytes) = key_encoding::encode_key_for_pty(&event, &self.current_modifiers) {
                    if let Some(ref mgr) = self.pty_manager {
                        if let Err(e) = mgr.write(surface_id, bytes) {
                            tracing::warn!(?surface_id, "PTY write failed: {e}");
                        }
                    }
                }
            }
            KeyRoute::ForwardToSidebar => {
                // Sidebar key handling deferred to egui integration.
            }
            KeyRoute::Unhandled => {}
        }
    }
}
```

**New method on `VeilApp`:**

```rust
fn execute_effect(&mut self, effect: ActionEffect) {
    match effect {
        ActionEffect::SpawnPty { surface_id, working_directory } => {
            if let Some(ref mut mgr) = self.pty_manager {
                let config = PtyConfig { command: None, args: vec![], working_directory: Some(working_directory), env: vec![], size: PtySize::default() };
                if let Err(e) = mgr.spawn(surface_id, config) {
                    tracing::error!(?surface_id, "failed to spawn PTY: {e}");
                }
            }
        }
        ActionEffect::ClosePty { surface_id } => {
            if let Some(ref mut mgr) = self.pty_manager {
                if let Err(e) = mgr.close(surface_id) {
                    tracing::warn!(?surface_id, "failed to close PTY: {e}");
                }
            }
        }
        ActionEffect::Redraw => {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }
        ActionEffect::None => {}
    }
}
```

**Tests:**

This unit is thin integration glue. The logic is in Units 1-4 which are independently testable. Event loop wiring is validated through the acceptance criteria (manual `cargo run` testing). One possible end-to-end test:

1. **Full pipeline test (no window):** Construct winit-compatible key event data manually, run it through translate -> route -> dispatch, assert the resulting effects and state changes are correct. This exercises the full path without requiring a real window.

## File Changes Summary

| File | Change |
|------|--------|
| `crates/veil/src/key_translation.rs` | New module: `translate_modifiers`, `translate_key_event` |
| `crates/veil/src/key_encoding.rs` | New module: `encode_key_for_pty` |
| `crates/veil/src/action_dispatch.rs` | New module: `ActionEffect` enum, `dispatch_action` function |
| `crates/veil/src/main.rs` | Add `mod` declarations. Rename `_keybindings` -> `keybindings`. Add `current_modifiers` field. Add `ModifiersChanged` + `KeyboardInput` arms. Add `execute_effect` method. New imports. |
| `crates/veil-core/src/workspace.rs` | Add `pane_id_for_surface` to `PaneNode` and `Workspace` |
| `crates/veil-core/src/keyboard.rs` | Fix bracket bindings from `Key::Named` to `Key::Character` in `with_defaults()`. Update tests. |

## Acceptance Criteria

1. **Typing sends keystrokes to PTY.** Characters, Enter, Tab, Backspace, arrow keys, and Ctrl+C all reach the PTY as correct byte sequences. Verifiable by running `cat` (once output wiring exists) or via tracing logs / `dtruss` on the child process.
2. **Cmd+D splits the pane.** The workspace layout changes from 1 to 2 panes. Two cell background regions appear with a divider between them. A new PTY is spawned for the new pane.
3. **Cmd+B toggles the sidebar gap.** `sidebar.visible` flips. Terminal area expands/contracts.
4. **Cmd+W closes a pane.** After splitting, Cmd+W removes the focused pane, remaining pane fills the area, and focus transfers to the remaining pane.
5. **Cmd+W is a no-op on last pane of only workspace.** No crash, no workspace removal.
6. **Modifier-only keys produce no PTY input.** Pressing Cmd, Shift, Ctrl, or Alt alone does not send bytes to the PTY.
7. **Key actions do not leak to PTY.** When Cmd+D is pressed, `'d'` is NOT sent to the PTY.
8. **Arrow keys produce ANSI escape sequences.** Verified by byte content in PTY write calls.
9. **Ctrl+C sends 0x03.** Verified by byte content.
10. **Quality gate passes.** `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`.
11. **No crashes on rapid key input.** Holding a key does not panic or deadlock.

## Dependencies

- **No new crate dependencies.** All types come from existing `winit`, `veil-core`, and `veil-pty` dependencies.
- **winit types used:** `KeyEvent`, `ElementState`, `keyboard::Key`, `keyboard::NamedKey`, `keyboard::ModifiersState`, `keyboard::SmolStr` (re-exported by winit).
- **veil-core APIs used as-is:** `route_key_event`, `KeybindingRegistry::lookup`, `AppState` methods, `FocusManager` methods, `compute_layout`, `find_pane_in_direction`.
- **veil-pty APIs used as-is:** `PtyManager::write`/`spawn`/`close`, `PtyConfig`, `PtySize`.
- **New method on existing type:** `PaneNode::pane_id_for_surface` and `Workspace::pane_id_for_surface` added to `veil-core`.

## Risks and Edge Cases

1. **IME input / dead keys:** winit 0.30's `KeyEvent.text` handles composed input. The encoding function should prefer `event.text` when available for printable input. Full IME support (composition events) is deferred.
2. **macOS Cmd key vs Ctrl:** On macOS, Cmd is the `logo` modifier, Ctrl is `ctrl`. `encode_key_for_pty` only generates control codes for `ctrl`, never for `logo`. This means Cmd+C does not send `\x03` -- it's either a keybinding (if bound) or passes through as a regular character.
3. **Key repeat:** winit sends repeated `KeyboardInput` events for held keys. All repeats are processed -- text repeats write to PTY, action repeats execute the action again. This matches terminal emulator behavior.
4. **Focus correctness after operations:** After split, new pane gets focus. After close, sibling gets focus. If focus points to a stale surface, PTY writes fail gracefully (`PtyError::Closed`).
5. **Thread safety:** All keyboard handling runs on the winit event loop thread. `AppState`, `FocusManager`, and `PtyManager` are owned by `VeilApp` and accessed exclusively from this thread. No synchronization needed.
6. **Bracket/brace keys with modifiers:** On some keyboard layouts, `[` requires Shift or Option. The translation uses `logical_key` (layout-aware), not `physical_key`.
7. **`ClosePane` on last pane:** If it's the only workspace, do nothing. If other workspaces exist, could close the workspace. The spec chooses no-op for safety -- closing the last workspace is a `CloseWorkspace` action.
8. **`SwitchWorkspace(n)` indexing:** 1-indexed, matching `Cmd+1` through `Cmd+9`. `SwitchWorkspace(1)` is the first workspace. Out-of-range is a no-op.

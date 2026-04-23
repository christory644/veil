# VEI-75: Wire Keyboard Input to PTY and Key Action Dispatch

## Context

The app can bootstrap a workspace, spawn a PTY, and render the terminal area with cell backgrounds, cursor, and focus border (VEI-74). But no keyboard input reaches the PTY, and no key actions are dispatched. The `_keybindings` field on `VeilApp` is underscore-prefixed (unused), and the `window_event` handler's match arm ignores all keyboard events via the `_ => {}` catch-all.

This task wires winit keyboard events through the existing key routing infrastructure (`KeybindingRegistry`, `FocusManager`, `route_key_event`) to either:

1. **Dispatch a key action** (split pane, close pane, toggle sidebar, etc.) -- if the key matches a registered binding
2. **Forward text input to the focused PTY** -- if no binding matches and a terminal surface has focus

After this, typing in the window sends keystrokes to the shell, and keyboard shortcuts (Cmd+D, Cmd+B, Cmd+W) trigger their respective actions with visible effects (new pane regions, sidebar gap toggling, pane removal).

### What exists

- **`KeybindingRegistry::with_defaults()`** -- already populated with all default bindings (Cmd+D, Cmd+B, Cmd+W, Cmd+N, etc.). Has `lookup(&KeyInput) -> Option<&KeyAction>`.
- **`FocusManager`** -- tracks focus target (`Surface(SurfaceId)` or `Sidebar`). Has `focused_surface() -> Option<SurfaceId>`.
- **`route_key_event(input, registry, focus) -> KeyRoute`** -- checks bindings first, then routes to focus target. Returns `KeyRoute::Action(action)`, `KeyRoute::ForwardToSurface(id)`, `KeyRoute::ForwardToSidebar`, or `KeyRoute::Unhandled`.
- **`KeyAction` enum** -- all action variants: `SplitHorizontal`, `SplitVertical`, `ClosePane`, `FocusNextPane`, `FocusPreviousPane`, `ToggleSidebar`, `ZoomPane`, `CreateWorkspace`, `SwitchWorkspace(n)`, etc.
- **`KeyInput`** -- domain type with `Key` (Character or Named) and `Modifiers` (ctrl, shift, alt, logo).
- **`PtyManager::write(surface_id, data)`** -- sends bytes to a surface's PTY via the write channel.
- **`AppState::split_pane(ws_id, pane_id, direction)`** -- splits a pane, returns `(PaneId, SurfaceId)`.
- **`AppState::close_pane(ws_id, pane_id)`** -- removes a pane, returns the closed `SurfaceId`.
- **`AppState::toggle_sidebar()`** -- flips `sidebar.visible`.
- **`AppState::toggle_zoom(ws_id, pane_id)`** -- toggles zoom state.
- **`AppState::create_workspace(name, cwd)`** -- creates a new workspace.
- **`AppState::set_active_workspace(ws_id)`** -- switches active workspace.
- **`PtyManager::spawn(surface_id, config)`** -- spawns a new PTY for a surface.
- **`PtyManager::close(surface_id)`** -- shuts down and removes a PTY.
- **`veil_core::navigation::find_pane_in_direction(panes, focused, direction)`** -- spatial pane navigation.
- **`veil_core::layout::compute_layout(root, available, zoomed)`** -- computes pane pixel rects (needed for directional navigation).

### What's missing

1. **winit-to-domain key translation** -- Converting `winit::event::WindowEvent::KeyboardInput` (and `ModifiersChanged`) into our `KeyInput` domain type. winit provides `KeyEvent` with `logical_key` and `state` (pressed/released); we need to translate `SmolStr` text / `NamedKey` variants into `Key::Character(char)` / `Key::Named(String)` and combine with tracked `Modifiers`.
2. **Event handler wiring** -- The `window_event` match needs arms for `KeyboardInput` and `ModifiersChanged`.
3. **Action dispatcher** -- A function that takes a `KeyAction` and the current state, performs the action (split, close, toggle, etc.), and returns any side effects (PTY spawn, PTY close, focus change).
4. **Text-to-bytes encoding** -- Converting character key presses into bytes for PTY write. For now, simple UTF-8 encoding of the character. Special keys (Enter, Tab, Escape, arrows, etc.) need their ANSI/VT escape sequences.

## Implementation Units

### Unit 1: winit key event translation (`key_translation` module)

**Location:** `crates/veil/src/key_translation.rs` (new module)

**What it does:**

Converts winit's `KeyEvent` and modifier state into the domain `KeyInput` type used by `KeybindingRegistry::lookup` and `route_key_event`.

**Functions:**

```rust
/// Convert a winit KeyEvent into a domain KeyInput.
/// Returns None for key releases, repeat events we want to skip, or
/// keys we can't meaningfully translate.
pub fn translate_key_event(event: &KeyEvent, modifiers: &Modifiers) -> Option<KeyInput>
```

Translation rules:
- Only process `ElementState::Pressed` events (not releases)
- `Key::Character(ref text)` with a single char -> `keyboard::Key::Character(c)` (lowercase the char when logo/ctrl modifier is held, since winit may report 'D' when Cmd+Shift+D is pressed but 'd' for Cmd+D -- we need to normalize)
- `Key::Named(named)` -> `keyboard::Key::Named(name.to_string())` mapping the winit `NamedKey` variants to string names ("Enter", "Tab", "Escape", "Backspace", "ArrowUp", etc.)
- Modifier-only key presses (Shift alone, Ctrl alone) -> return `None`

```rust
/// Convert a winit ModifiersState into our domain Modifiers.
pub fn translate_modifiers(state: &winit::event::Modifiers) -> keyboard::Modifiers
```

Maps `state.state()` flags to `keyboard::Modifiers { ctrl, shift, alt, logo }`.

```rust
/// Encode a key event as bytes to send to the PTY.
/// Returns None if the key has no byte representation (e.g., modifier-only keys).
pub fn key_to_pty_bytes(event: &KeyEvent, modifiers: &Modifiers) -> Option<Vec<u8>>
```

Encoding rules:
- `Key::Character(ref text)` -> text as UTF-8 bytes (this handles multi-byte chars)
- BUT: if ctrl is held and the character is a-z, encode as control character (char - 0x60, i.e., Ctrl+C = 0x03)
- `Key::Named(NamedKey::Enter)` -> `\r` (0x0D)
- `Key::Named(NamedKey::Tab)` -> `\t` (0x09)
- `Key::Named(NamedKey::Escape)` -> `\x1b` (0x1B)
- `Key::Named(NamedKey::Backspace)` -> `\x7f` (0x7F, matching most terminal emulators)
- `Key::Named(NamedKey::ArrowUp)` -> `\x1b[A`
- `Key::Named(NamedKey::ArrowDown)` -> `\x1b[B`
- `Key::Named(NamedKey::ArrowRight)` -> `\x1b[C`
- `Key::Named(NamedKey::ArrowLeft)` -> `\x1b[D`
- `Key::Named(NamedKey::Home)` -> `\x1b[H`
- `Key::Named(NamedKey::End)` -> `\x1b[F`
- `Key::Named(NamedKey::Delete)` -> `\x1b[3~`
- `Key::Named(NamedKey::PageUp)` -> `\x1b[5~`
- `Key::Named(NamedKey::PageDown)` -> `\x1b[6~`
- Other named keys -> `None` (handled later via libghosty input encoding)

Note: This is a minimal encoding sufficient to make the terminal interactive. Full VT input encoding (Kitty keyboard protocol, function keys, mouse events) will come when libghosty input encoding is integrated.

**Tests:**

All tests in this unit are pure logic tests -- no GPU, no window, no PTY.

1. **Character key translation:** `translate_key_event` with a character key and no modifiers returns `Some(KeyInput)` with `Key::Character('a')`.
2. **Named key translation:** `translate_key_event` with Enter returns `Some(KeyInput)` with `Key::Named("Enter")`.
3. **Key release ignored:** `translate_key_event` with `ElementState::Released` returns `None`.
4. **Modifier-only key ignored:** `translate_key_event` with just Shift pressed returns `None`.
5. **Modifiers correctly mapped:** `translate_modifiers` with logo flag set produces `Modifiers { logo: true, .. }`.
6. **Character PTY bytes:** `key_to_pty_bytes` for 'a' returns `Some(vec![0x61])`.
7. **Enter PTY bytes:** `key_to_pty_bytes` for Enter returns `Some(vec![0x0D])`.
8. **Backspace PTY bytes:** `key_to_pty_bytes` for Backspace returns `Some(vec![0x7F])`.
9. **Arrow key PTY bytes:** `key_to_pty_bytes` for ArrowUp returns `Some(vec![0x1B, 0x5B, 0x41])`.
10. **Ctrl+C PTY bytes:** `key_to_pty_bytes` for Ctrl+C returns `Some(vec![0x03])`.
11. **Tab PTY bytes:** `key_to_pty_bytes` for Tab returns `Some(vec![0x09])`.
12. **Escape PTY bytes:** `key_to_pty_bytes` for Escape returns `Some(vec![0x1B])`.
13. **Multi-byte UTF-8:** `key_to_pty_bytes` for a non-ASCII character (e.g., 'é') returns correct UTF-8 bytes.
14. **Logo modifier alone returns None from key_to_pty_bytes:** When logo is held with no character, no bytes emitted.

### Unit 2: Action dispatcher (`action_dispatch` module)

**Location:** `crates/veil/src/action_dispatch.rs` (new module)

**What it does:**

Takes a `KeyAction` and the current application state, performs the action, and returns side effects that the event loop needs to execute (PTY spawn/close, focus updates). This is a pure-logic module that operates on `AppState`, `FocusManager`, and layout data -- no direct PTY or window interaction.

**Types:**

```rust
/// Side effects produced by dispatching a key action.
/// The event loop reads these and executes them.
#[derive(Debug, PartialEq)]
pub enum ActionEffect {
    /// Spawn a new PTY for the given surface in the given working directory.
    SpawnPty { surface_id: SurfaceId, working_directory: PathBuf },
    /// Close the PTY for the given surface.
    ClosePty { surface_id: SurfaceId },
    /// Request a redraw (layout changed).
    Redraw,
    /// No effect (action was a no-op or not applicable).
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

Action handling:

- **`SplitHorizontal` / `SplitVertical`:** Requires active workspace + focused pane. Calls `app_state.split_pane(ws_id, pane_id, direction)`. On success, returns `[SpawnPty { surface_id, cwd }, Redraw]`. Focus stays on the original pane (the user can navigate to the new one).
- **`ClosePane`:** Requires active workspace + focused pane. If this is the last pane in the only workspace, ignore (don't close -- user has nowhere to go). Otherwise calls `app_state.close_pane(ws_id, pane_id)`. On success, moves focus to a sibling pane and returns `[ClosePty { surface_id }, Redraw]`. If this was the last pane in the workspace but other workspaces exist, close the workspace entirely.
- **`FocusNextPane` / `FocusPreviousPane`:** Get the list of surface IDs from the active workspace's layout tree. Find the current focused surface's index, advance/retreat circularly, update focus. Returns `[Redraw]`.
- **`FocusPaneLeft/Right/Up/Down`:** Compute pane layouts via `compute_layout`, find the focused pane's `PaneId`, call `find_pane_in_direction`. If found, update focus to that pane's surface. Returns `[Redraw]` or `[None]` if at edge.
- **`ToggleSidebar`:** `app_state.toggle_sidebar()`. Returns `[Redraw]`.
- **`ZoomPane`:** Requires active workspace + focused pane. `app_state.toggle_zoom(ws_id, pane_id)`. Returns `[Redraw]`.
- **`CreateWorkspace`:** `app_state.create_workspace(name, cwd)` using the active workspace's cwd (or fallback). Set active. Focus the new workspace's root surface. Returns `[SpawnPty { surface_id, cwd }, Redraw]`.
- **`SwitchWorkspace(n)`:** Look up the nth workspace (1-indexed). If it exists, `app_state.set_active_workspace(id)`. Focus the first surface in that workspace. Returns `[Redraw]`.
- **`CloseWorkspace`:** Close the active workspace. For each surface, produce a `ClosePty` effect. Activate an adjacent workspace and focus its root surface. Returns `[ClosePty for each surface, Redraw]`.
- **`SwitchToWorkspacesTab` / `SwitchToConversationsTab`:** `app_state.set_sidebar_tab(tab)`. Returns `[Redraw]`.
- **`FocusSidebar`:** `focus.focus_sidebar()`. Returns `[Redraw]`.
- **`FocusTerminal`:** Focus the first surface of the active workspace. Returns `[Redraw]`.
- **`RenameWorkspace`:** No-op for now (needs text input UI). Returns `[None]`.

Helper function to find the focused pane's `PaneId` from a `SurfaceId`:

```rust
/// Find the PaneId that corresponds to a SurfaceId in the given workspace layout.
fn surface_to_pane_id(layout: &PaneNode, surface_id: SurfaceId) -> Option<PaneId>
```

**Tests:**

All tests are pure-logic, operating on `AppState` and `FocusManager` directly.

1. **SplitHorizontal creates new pane:** Start with 1 pane, dispatch SplitHorizontal, assert workspace has 2 panes and effects include `SpawnPty`.
2. **SplitVertical creates new pane:** Same as above but vertical.
3. **SplitHorizontal with no focus returns no effects:** Dispatch with no focused surface, assert empty effects.
4. **ClosePane removes pane:** Start with 2 panes, focus one, dispatch ClosePane, assert workspace has 1 pane and effects include `ClosePty`.
5. **ClosePane on last pane of only workspace is no-op:** Start with 1 pane, 1 workspace, dispatch ClosePane, assert no change.
6. **ClosePane moves focus to sibling:** Start with 2 panes, close the focused one, assert focus moved to the remaining pane.
7. **FocusNextPane cycles forward:** With 3 panes focused on the first, dispatch FocusNextPane, assert focus moved to second.
8. **FocusNextPane wraps around:** Focused on the last pane, FocusNextPane wraps to first.
9. **FocusPreviousPane wraps around:** Focused on the first pane, wraps to last.
10. **FocusPaneRight finds right neighbor:** Two horizontal panes, focused left, dispatch FocusPaneRight, focus moves right.
11. **FocusPaneRight at edge is no-op:** Focused on rightmost pane, returns no effective change.
12. **ToggleSidebar flips visibility:** Assert sidebar visible changes.
13. **ZoomPane toggles zoom state:** Assert workspace.zoomed_pane changes.
14. **CreateWorkspace adds workspace and spawns PTY:** Assert workspace count increases and effects include SpawnPty.
15. **SwitchWorkspace changes active:** With 3 workspaces, SwitchWorkspace(2) activates the second.
16. **SwitchWorkspace out of range is no-op:** SwitchWorkspace(9) with only 2 workspaces does nothing.
17. **CloseWorkspace removes workspace:** With 2 workspaces, close active, assert count is 1 and ClosePty effects emitted for all surfaces.
18. **SwitchToWorkspacesTab changes tab:** Assert sidebar tab changes.
19. **SwitchToConversationsTab changes tab:** Assert sidebar tab changes.
20. **FocusSidebar changes focus target:** Assert focus target is Sidebar.
21. **FocusTerminal returns focus to surface:** After focusing sidebar, dispatch FocusTerminal, assert surface is focused.
22. **Dispatch with no active workspace returns empty effects:** Fresh AppState, no workspace, all actions return empty.

### Unit 3: Event loop wiring

**Location:** `crates/veil/src/main.rs`

**What it does:**

Adds `ModifiersChanged` and `KeyboardInput` handling to the `window_event` method. This is the integration point that connects Units 1 and 2 to the winit event loop.

**Changes to `VeilApp` struct:**

```rust
struct VeilApp {
    // ... existing fields ...
    // Remove underscore prefix from _keybindings:
    keybindings: KeybindingRegistry,
    // Add tracked modifier state:
    current_modifiers: keyboard::Modifiers,
}
```

**New match arms in `window_event`:**

```rust
WindowEvent::ModifiersChanged(new_modifiers) => {
    self.current_modifiers = translate_modifiers(&new_modifiers);
}

WindowEvent::KeyboardInput { event, .. } => {
    if event.state != ElementState::Pressed {
        return;
    }
    if let Some(key_input) = translate_key_event(&event, &self.current_modifiers) {
        let route = route_key_event(&key_input, &self.keybindings, &self.focus);
        match route {
            KeyRoute::Action(action) => {
                let effects = dispatch_action(
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
                if let Some(bytes) = key_to_pty_bytes(&event, &self.current_modifiers) {
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
/// Execute a side effect produced by action dispatch.
fn execute_effect(&mut self, effect: ActionEffect) {
    match effect {
        ActionEffect::SpawnPty { surface_id, working_directory } => {
            if let Some(ref mut mgr) = self.pty_manager {
                let config = PtyConfig {
                    command: None,
                    args: vec![],
                    working_directory: Some(working_directory),
                    env: vec![],
                    size: PtySize::default(),
                };
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

Unit 3 is integration glue. The individual components (translation, dispatch) are thoroughly tested in Units 1 and 2. The wiring in `main.rs` is minimal branching and is verified through the acceptance criteria (manual + `cargo run`).

One testable aspect: the `execute_effect` method could be tested if we extracted it to a separate module, but since it's just forwarding to PtyManager methods that are already tested, the value is low. The event loop wiring is validated by the acceptance criteria below.

## File Changes Summary

| File | Change |
|------|--------|
| `crates/veil/src/key_translation.rs` | New module: winit-to-domain key translation and PTY byte encoding |
| `crates/veil/src/action_dispatch.rs` | New module: action dispatcher with `ActionEffect` type |
| `crates/veil/src/main.rs` | Add `mod key_translation; mod action_dispatch;` declarations. Remove `_` prefix from `_keybindings`. Add `current_modifiers` field. Add `ModifiersChanged` + `KeyboardInput` arms to `window_event`. Add `execute_effect` method. |

## Acceptance Criteria

1. **Typing sends keystrokes to PTY:** Characters typed in the window are received by the shell process (verifiable via `strace`/`dtruss` or by checking that the shell's PTY slave receives bytes, even though rendered output is not yet visible).
2. **Cmd+D splits pane:** Pressing Cmd+D produces two cell background regions with a divider visible between them. The original pane retains focus.
3. **Cmd+B toggles sidebar:** Pressing Cmd+B toggles the sidebar gap -- terminal area expands/contracts.
4. **Cmd+W closes pane:** After splitting with Cmd+D, pressing Cmd+W in one pane removes it and the layout returns to a single pane. Focus moves to the remaining pane.
5. **Cmd+W does not close last pane of only workspace:** With a single pane and single workspace, Cmd+W is a no-op.
6. **Cmd+N creates new workspace:** A new workspace is created and becomes active (visible as a fresh single-pane layout with a new PTY).
7. **Enter, Tab, Backspace work:** These special keys produce their correct byte sequences.
8. **Ctrl+C sends interrupt:** Ctrl+C sends byte 0x03 to the PTY.
9. **Arrow keys produce escape sequences:** Arrow keys send the correct ANSI sequences.
10. **`cargo clippy --all-targets --all-features -- -D warnings`** passes.
11. **`cargo test`** passes (existing tests plus all new unit tests).
12. **No crashes on rapid key input:** Holding a key down does not crash or deadlock.

## Dependencies

- **No new crate dependencies.** All required types and APIs already exist across `veil`, `veil-core`, and `veil-pty`.
- **winit types used:** `KeyEvent`, `ElementState`, `key::Key`, `key::NamedKey`, `event::Modifiers` -- all from the existing `winit` dependency.
- **veil-core types used:** `KeyInput`, `Key`, `Modifiers`, `KeyAction`, `KeybindingRegistry`, `FocusManager`, `FocusTarget`, `KeyRoute`, `route_key_event`, `AppState`, `SidebarTab`, `SplitDirection`, `PaneNode`, `PaneId`, `SurfaceId`, `WorkspaceId`, `compute_layout`, `Rect`, `find_pane_in_direction`, `Direction`.
- **veil-pty types used:** `PtyManager`, `PtyConfig`, `PtySize`.

## Risks and Edge Cases

1. **winit `logical_key` vs `physical_key`:** We use `logical_key` (which respects keyboard layout) for character input. This means non-QWERTY layouts work correctly for text input, but keybindings like Cmd+D refer to the logical 'D' key, not the physical key position. This matches most terminal emulators' behavior.
2. **Key repeat:** winit sends repeated `KeyboardInput` events when a key is held. We process all of them (both for text forwarding and action dispatch). For actions like SplitHorizontal, repeated dispatch creates multiple splits rapidly -- this is acceptable behavior, matching how tmux handles repeated prefix+split.
3. **Modifier key translation on macOS vs Linux:** On macOS, `logo` maps to Cmd. On Linux, `logo` maps to Super/Win key. The existing `KeybindingRegistry::with_defaults()` uses `logo` for all Cmd-equivalent bindings, which is correct for both platforms.
4. **IME composition:** This minimal implementation does not handle IME input (for CJK text entry). winit has `Ime` events for this. Deferred to a later issue.
5. **Ctrl+key encoding ambiguity:** Some Ctrl+key combinations (like Ctrl+H = backspace, Ctrl+I = tab) overlap with named keys. We handle this by checking the `Key::Character` variant with ctrl modifier and encoding as control characters. The named key variants (Tab, Backspace) are handled separately in `key_to_pty_bytes`.
6. **Focus race during split:** When splitting, we spawn a PTY for the new surface but keep focus on the original. The new PTY's shell starts writing output to its terminal, but since it doesn't have focus, no keyboard input goes to it until the user navigates. This is correct behavior.
7. **Close pane focus transfer:** When closing the focused pane, we need to pick a sibling to receive focus. The dispatch function uses the workspace's pane list to find the next available surface.

# VEI-12: Navigation Pane -- egui Sidebar with Workspaces Tab

## Context

Veil's navigation pane is the left sidebar that provides workspace awareness and conversation history browsing. This task delivers the first visible UI beyond the terminal rendering pipeline: an egui-powered sidebar integrated into the existing wgpu render loop, showing the Workspaces tab with live workspace data from `AppState`.

The sidebar is a core P0 feature (see PRD items 2-3). The UI design doc specifies a fixed-width left panel with two tabs (Workspaces / Conversations) and per-workspace metadata rendering. The system design doc specifies egui as the sidebar technology, rendered as a pass within the existing wgpu pipeline.

### What already exists

- **`veil-core::state::AppState`** -- Central state with `workspaces: Vec<Workspace>`, `active_workspace_id`, `sidebar: SidebarState` (visible, active_tab, width_px). Methods: `toggle_sidebar()`, `set_sidebar_tab()`, `set_active_workspace()`, `create_workspace()`.
- **`veil-core::state::SidebarTab`** -- Enum with `Workspaces` and `Conversations` variants.
- **`veil-core::state::SidebarState`** -- Struct with `visible: bool`, `active_tab: SidebarTab`, `width_px: u32`.
- **`veil-core::workspace::Workspace`** -- Has `id`, `name`, `working_directory`, `layout` (PaneNode tree), `branch: Option<String>`, `zoomed_pane`.
- **`veil-core::focus`** -- `FocusManager` with `FocusTarget::Sidebar` variant. `route_key_event` already forwards keys to sidebar when sidebar is focused.
- **`veil/src/renderer.rs`** -- wgpu renderer with device, queue, surface, pipeline. Renders `FrameGeometry` (colored quads) each frame.
- **`veil/src/frame.rs`** -- `build_frame_geometry()` composes terminal pane quads, respecting `sidebar.visible` and `sidebar.width_px` for the terminal area offset.
- **`veil/src/main.rs`** -- winit `ApplicationHandler` with `resumed` (creates window + renderer), `window_event` (handles resize, redraw, close). Calls `build_frame_geometry` + `renderer.render()` on redraw.
- **`veil-ui/src/lib.rs`** -- Empty crate stub (`//! Navigation pane and sidebar UI for Veil.`), depends on `veil-core`.

### What this task delivers

1. egui integrated into the existing wgpu render pipeline, rendering alongside terminal quads in the same frame.
2. A sidebar container with fixed width, toggle visibility, and a tab header bar.
3. The Workspaces tab rendering a scrollable list of workspace entries with metadata.
4. Click-to-switch-workspace interaction.

### What is explicitly out of scope

- Drag-to-reorder workspaces
- Right-click context menus
- Keyboard navigation within sidebar (j/k)
- Theming (dark/light/system, accent colors)
- Cmd+N shortcut for new workspace
- Conversations tab content (rendered as a placeholder "Coming soon" label)

### Key design decisions

**egui renders in its own render pass, after the terminal pass.** The existing renderer draws terminal quads (cell backgrounds, cursors, dividers, focus borders) in a single render pass. egui gets its own pass via `egui-wgpu`'s `Renderer`, which manages its own pipeline, textures, and draw calls. Both passes write to the same surface texture. The terminal pass clears the background; the egui pass composites on top (the sidebar region). This separation means the two pipelines don't share vertex formats or shaders.

**Sidebar UI logic lives in `veil-ui`, not in the `veil` binary.** The `veil-ui` crate contains pure egui layout code that takes `&AppState` and an `&egui::Context` and emits UI. It returns a `SidebarResponse` describing user interactions (tab switch, workspace click). The `veil` binary owns the egui integration plumbing (egui-wgpu renderer, egui-winit event handling) and calls into `veil-ui` each frame.

**egui-winit handles input translation.** The `egui_winit::State` translates winit events into egui input. This runs before our custom key routing, so egui gets first crack at events when the sidebar is visible. We gate this: only feed events to egui when `sidebar.visible` is true, and only consume them (preventing pass-through to terminal) when egui reports `wants_pointer_input` or `wants_keyboard_input`.

**Workspace entry rendering is a standalone function.** Each workspace entry is rendered by a function that takes `&Workspace`, `is_active: bool`, and a notification count. This makes it independently testable (egui provides `Context::new()` for headless testing).

## Implementation Units

### Unit 1: egui-wgpu Integration (`veil` binary crate)

Wire `egui-wgpu` and `egui-winit` into the existing renderer and event loop so egui can draw to the window.

**New dependencies (workspace-level `Cargo.toml`):**

```toml
egui = "0.34"
egui-wgpu = "0.34"
egui-winit = { version = "0.34", default-features = false }
```

`egui-winit` should have `default-features = false` to avoid pulling in clipboard/link dependencies we don't need yet. On macOS this avoids unnecessary Wayland/X11 feature flags.

**Changes to `veil/Cargo.toml`:** Add `egui`, `egui-wgpu`, `egui-winit` dependencies.

**Changes to `veil-ui/Cargo.toml`:** Add `egui` dependency (the UI crate uses egui types but not the wgpu/winit integration crates).

**Changes to `veil/src/renderer.rs`:**

Add an `EguiIntegration` struct (or fields on `Renderer`) that owns:
- `egui::Context` -- the egui context (created once, reused every frame)
- `egui_winit::State` -- translates winit events to egui raw input
- `egui_wgpu::Renderer` -- renders egui paint output to wgpu

New methods on `Renderer`:
- `init_egui(window: &Window, device: &Device, surface_format: TextureFormat) -> EguiIntegration` -- create egui context, winit state, and wgpu renderer.
- `render_egui(integration: &mut EguiIntegration, encoder: &mut CommandEncoder, view: &TextureView, screen_descriptor: ScreenDescriptor, full_output: FullOutput)` -- run the egui wgpu renderer for one frame.

**Changes to `veil/src/main.rs`:**

In `VeilApp`:
- Add `egui_integration: Option<EguiIntegration>` field.
- In `resumed`: after creating `Renderer`, call `init_egui`.
- In `window_event`: before our own event handling, pass winit events to `egui_winit::State::on_window_event()`. Track whether egui consumed the event.
- In `RedrawRequested`:
  1. Call `egui_integration.ctx.begin_pass(raw_input)`.
  2. Call sidebar UI function (Unit 3).
  3. Call `egui_integration.ctx.end_pass()`.
  4. Render terminal quads (existing `renderer.render()`).
  5. Render egui output (new egui render pass).
  6. Present.

**Test strategy:**

This unit is GPU-dependent, so most tests are `#[ignore]` integration tests. The structural correctness is verified via the downstream units.

- **Compile test:** `cargo build -p veil` succeeds with new dependencies.
- **Ignored integration test:** Create renderer + egui integration on a headless wgpu device (if available), run one frame with empty egui output. Verify no panics.
- **Event gating test (unit, no GPU):** Verify that winit events are only forwarded to egui when `sidebar.visible` is true. This can be tested by checking that `egui_winit::State::on_window_event` is called conditionally based on sidebar state (tested via a mock or by checking the response).

### Unit 2: Sidebar Container (`veil-ui` crate)

The structural frame: a fixed-width panel on the left side of the window with a tab header bar. This is the egui layout code that defines the sidebar's shape.

**File:** `crates/veil-ui/src/sidebar.rs`

**New dependency on `veil-ui/Cargo.toml`:** `egui.workspace = true`

**Types:**

```rust
/// Response from the sidebar UI, describing user interactions.
#[derive(Debug, Default)]
pub struct SidebarResponse {
    /// User clicked a workspace to switch to it.
    pub switch_to_workspace: Option<WorkspaceId>,
    /// User clicked a tab to switch to it.
    pub switch_tab: Option<SidebarTab>,
}
```

**Functions:**

```rust
/// Render the sidebar container into the egui context.
///
/// Returns a `SidebarResponse` describing any user interactions.
/// The caller (the `veil` binary) interprets the response and
/// mutates `AppState` accordingly.
pub fn render_sidebar(ctx: &egui::Context, state: &AppState) -> SidebarResponse
```

**Layout:**

1. `egui::SidePanel::left("veil_sidebar")` with exact width `state.sidebar.width_px` pixels.
2. Inside the panel:
   a. **Tab header bar:** A horizontal strip at the top with two buttons ("Workspaces" / "Conversations"). The active tab gets a distinct visual treatment (e.g., underline or background highlight). Clicking a tab sets `SidebarResponse::switch_tab`.
   b. **Tab content area:** Below the header bar, a `ScrollArea::vertical()` that renders the content for the active tab. For `SidebarTab::Workspaces`, calls `render_workspaces_tab()` (Unit 3). For `SidebarTab::Conversations`, renders a placeholder label.

**Sidebar visibility:** The caller (`veil` binary) only calls `render_sidebar` when `state.sidebar.visible` is true. When hidden, no egui side panel is rendered and the terminal area fills the full window width.

**Test strategy:**

egui supports headless context creation via `egui::Context::default()`, which allows UI logic testing without a GPU.

Happy path:
- Render sidebar with `SidebarTab::Workspaces` active: verify the side panel is allocated (check via egui's `SidePanel` response, or by checking `ctx.used_rect()` width).
- Render sidebar with `SidebarTab::Conversations` active: verify no workspace entries rendered.
- Click on "Conversations" tab button: verify `SidebarResponse::switch_tab == Some(SidebarTab::Conversations)`.

Edge cases:
- Empty workspace list: sidebar renders without panic, no workspace entries shown.
- Sidebar width of 0: no panic, panel renders (egui handles gracefully).
- Very long workspace name: text is truncated (not tested explicitly, but verified visually; egui handles via `Label` ellipsis).

### Unit 3: Workspace List Rendering (`veil-ui` crate)

Render the workspace entries inside the Workspaces tab scroll area.

**File:** `crates/veil-ui/src/workspace_list.rs`

**Types:**

```rust
/// Data needed to render a single workspace entry.
/// This is a view-model extracted from AppState to keep rendering
/// decoupled from the full state.
pub struct WorkspaceEntryData<'a> {
    pub id: WorkspaceId,
    pub name: &'a str,
    pub working_directory: &'a Path,
    pub branch: Option<&'a str>,
    pub is_active: bool,
    pub notification_count: usize,
}
```

**Functions:**

```rust
/// Render the workspace list inside a ScrollArea.
/// Returns the ID of a workspace the user clicked to switch to, if any.
pub fn render_workspaces_tab(
    ui: &mut egui::Ui,
    entries: &[WorkspaceEntryData],
) -> Option<WorkspaceId>

/// Render a single workspace entry row.
/// Returns true if the user clicked this entry.
fn render_workspace_entry(
    ui: &mut egui::Ui,
    entry: &WorkspaceEntryData,
) -> bool
```

**Entry layout (per workspace):**

```
┌────────────────────────┐
│ ● api-server           │   <- active indicator + name (bold if active)
│   main                 │   <- git branch (dimmed)
│   ~/repos/api          │   <- abbreviated working directory (dimmed)
└────────────────────────┘
```

- **Active indicator:** `●` (Unicode bullet) for the active workspace, `○` (circle) for background workspaces.
- **Name:** Primary label. Bold weight for active workspace.
- **Git branch:** Secondary label in dimmed color. Omitted if `branch` is `None`.
- **Working directory:** Abbreviated path (replace home dir with `~`). Dimmed color.
- **Notification badge:** If `notification_count > 0`, show a small badge number next to the name.
- **Hover state:** egui's built-in `Response::hovered()` provides visual feedback via the frame.
- **Click interaction:** The entire entry is a clickable region. Clicking returns the workspace ID.

**Path abbreviation helper:**

```rust
/// Abbreviate a path by replacing the home directory prefix with "~".
fn abbreviate_path(path: &Path) -> String
```

**Extracting view data from AppState:**

A helper function in `veil-ui` (in `sidebar.rs` or a helper module) extracts `Vec<WorkspaceEntryData>` from `&AppState`:

```rust
/// Extract workspace entry view data from AppState.
pub fn extract_workspace_entries(state: &AppState) -> Vec<WorkspaceEntryData>
```

This counts unacknowledged notifications per workspace from `state.notifications`.

**Test strategy:**

Happy path:
- Render workspace list with 3 workspaces: verify 3 entries rendered (check via egui memory or response).
- Active workspace has `●` indicator and bold name.
- Inactive workspace has `○` indicator.
- Workspace with branch shows branch text.
- Workspace without branch omits branch line.
- Click on inactive workspace returns `Some(workspace_id)`.
- Click on active workspace returns `Some(workspace_id)` (no-op in caller, but click still registers).

Edge cases:
- Empty workspace list: renders nothing, returns `None`.
- Single workspace: renders one entry.
- Workspace with very long name: no panic, text truncates.
- Workspace with no working directory (empty `PathBuf`): renders empty path line.
- Path abbreviation: `/Users/chris/repos/api` becomes `~/repos/api`.
- Path abbreviation: non-home path `/tmp/test` stays as `/tmp/test`.

Notification badge:
- Workspace with 0 notifications: no badge shown.
- Workspace with 3 unacknowledged notifications: badge shows "3".

### Unit 4: Click-to-Switch and Event Wiring (`veil` binary crate)

Connect the `SidebarResponse` from the UI layer back to `AppState` mutations and the existing event/focus system.

**Changes to `veil/src/main.rs`:**

In the `RedrawRequested` handler, after calling `render_sidebar`:

```rust
let response = veil_ui::sidebar::render_sidebar(&egui_ctx, &self.app_state);

if let Some(tab) = response.switch_tab {
    self.app_state.set_sidebar_tab(tab);
}

if let Some(ws_id) = response.switch_to_workspace {
    if let Err(e) = self.app_state.set_active_workspace(ws_id) {
        tracing::warn!("failed to switch workspace: {e}");
    }
    // Update focus to the new workspace's first surface
    if let Some(ws) = self.app_state.workspace(ws_id) {
        if let Some(surface_id) = ws.layout.surface_ids().first() {
            self.focus.focus_surface(*surface_id);
        }
    }
}
```

**Toggle sidebar visibility:**

The existing `KeyAction::ToggleSidebar` (bound to Cmd+B) already exists in the keybinding registry. The event loop needs to handle it:

```rust
// In the key event handler:
KeyAction::ToggleSidebar => {
    self.app_state.toggle_sidebar();
    window.request_redraw();
}
```

**Tab switching via keyboard:**

The `KeyAction` enum already has `SwitchToWorkspacesTab` and `SwitchToConversationsTab` variants with default bindings (`Ctrl+Shift+W` and `Ctrl+Shift+C`). The event loop handler needs to wire these to `app_state.set_sidebar_tab()`:

```rust
KeyAction::SwitchToWorkspacesTab => {
    self.app_state.set_sidebar_tab(SidebarTab::Workspaces);
    window.request_redraw();
}
KeyAction::SwitchToConversationsTab => {
    self.app_state.set_sidebar_tab(SidebarTab::Conversations);
    window.request_redraw();
}
```

**Changes to `veil/src/frame.rs`:**

No changes needed. The existing `build_frame_geometry` already respects `sidebar.visible` and `sidebar.width_px` when computing the terminal area offset. The egui sidebar occupies the left region, and terminal quads start at `x = sidebar.width_px`.

**Test strategy:**

Happy path:
- `SidebarResponse` with `switch_to_workspace = Some(ws_id)`: verify `app_state.active_workspace_id` changes.
- `SidebarResponse` with `switch_tab = Some(Conversations)`: verify `app_state.sidebar.active_tab` changes.
- Toggle sidebar: verify `app_state.sidebar.visible` flips.
- Switch workspace: verify `focus.focused_surface()` updates to new workspace's first surface.

Edge cases:
- Switch to nonexistent workspace (stale ID from a race): `set_active_workspace` returns error, logged as warning, no crash.
- Toggle sidebar when already hidden: sidebar becomes visible.
- Switch tab when sidebar is hidden: tab changes but sidebar stays hidden (tab switch is recorded for when sidebar is shown).

Integration (ignored, needs window):
- Full redraw cycle with sidebar visible produces egui draw commands.
- Full redraw cycle with sidebar hidden produces no egui draw commands.

## Acceptance Criteria

1. `cargo build` succeeds for all workspace crates.
2. `cargo test` passes all tests (including new ones in `veil-ui` and `veil`).
3. `cargo clippy --all-targets --all-features -- -D warnings` passes.
4. `cargo fmt --check` passes.
5. When `sidebar.visible` is true, an egui side panel renders on the left side of the window at `sidebar.width_px` width.
6. The sidebar shows a tab header bar with "Workspaces" and "Conversations" buttons. Clicking switches the active tab.
7. The Workspaces tab renders a scrollable list of workspace entries with: active indicator (bullet), name, git branch (if set), abbreviated working directory.
8. Clicking a workspace entry in the list switches the active workspace and updates focus.
9. `Cmd+B` toggles sidebar visibility. Terminal area resizes to fill the freed space.
10. Notifications badge appears next to workspace entries with unacknowledged notifications.
11. The Conversations tab shows a placeholder label.
12. egui input events are only processed when the sidebar is visible, and only consumed (not forwarded to terminal) when egui indicates it wants them.

## Dependencies

### New crate dependencies (added to workspace `Cargo.toml`)

| Crate | Version | Purpose |
|-------|---------|---------|
| `egui` | `0.34` | Immediate-mode UI framework (types, layout, painting) |
| `egui-wgpu` | `0.34` | Renders egui output via wgpu (manages pipeline, textures) |
| `egui-winit` | `0.34` | Translates winit events into egui input |

Version 0.34 is the latest release. It depends on `wgpu >=24` (we have 24.0.5) and `winit >=0.30` (we have 0.30.13), so versions are compatible.

`egui-winit` should be added with `default-features = false` at the workspace level to avoid unnecessary platform clipboard/link dependencies.

### New files

| File | Purpose |
|------|---------|
| `crates/veil-ui/src/sidebar.rs` | Sidebar container layout, tab header, `SidebarResponse`, `render_sidebar()` |
| `crates/veil-ui/src/workspace_list.rs` | Workspace entry rendering, `render_workspaces_tab()`, path abbreviation |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Add `egui`, `egui-wgpu`, `egui-winit` to `[workspace.dependencies]` |
| `crates/veil/Cargo.toml` | Add `egui`, `egui-wgpu`, `egui-winit` dependencies |
| `crates/veil-ui/Cargo.toml` | Add `egui` dependency |
| `crates/veil-ui/src/lib.rs` | Add `pub mod sidebar;` and `pub mod workspace_list;` |
| `crates/veil/src/renderer.rs` | Add egui wgpu renderer, `EguiIntegration` struct, egui render pass |
| `crates/veil/src/main.rs` | Wire egui event handling, call sidebar UI each frame, handle `SidebarResponse` |
| `crates/veil-core/src/keyboard.rs` | No changes needed -- `SwitchToWorkspacesTab`, `SwitchToConversationsTab`, and `ToggleSidebar` already exist with default bindings |

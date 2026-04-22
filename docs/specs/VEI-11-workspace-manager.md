# VEI-11: Workspace Manager -- Layout, Navigation, Zoom, Rename, Reorder

## Context

Veil's workspace manager is the core "userspace" terminal environment. A workspace contains a binary tree of panes (the `PaneNode` tree), and each pane renders a terminal surface. VEI-10 delivered the workspace/pane data structure, split/close operations, keyboard dispatch, focus tracking, and `AppState` CRUD. What remains is the geometry and interaction layer that makes these structures usable in a real GUI:

1. **Layout calculation** -- Translating the abstract `PaneNode` tree into concrete pixel rectangles that the renderer can use to position terminal surfaces on screen.
2. **Directional pane navigation** -- Moving focus between panes spatially (left/right/up/down via Ctrl+h/j/k/l), not just sequentially (next/prev).
3. **Zoom/unzoom** -- Temporarily expanding a single pane to fill the entire terminal area, then restoring the split layout.
4. **Workspace rename** -- Changing a workspace's display name.
5. **Workspace reorder** -- Moving workspaces to different positions in the list (for sidebar ordering and Cmd+1-9 assignment).

Items explicitly NOT in scope (tracked separately):
- VEI-30: Workspace metadata enrichment (git branch, ports, PR status, agent indicator)
- VEI-31: Pane resize (keyboard and drag-based pane ratio adjustment)
- VEI-32: Workspace persistence lifecycle (auto-save on exit, restore on launch)

### What already exists

- `veil-core::workspace` -- `WorkspaceId`, `PaneId`, `SurfaceId`, `SplitDirection`, `PaneNode` (binary tree with `Leaf`/`Split` variants), `Workspace` (struct with `split_pane`, `close_pane`, `pane_ids`), `WorkspaceError`
- `veil-core::state::AppState` -- workspace CRUD (`create_workspace`, `close_workspace`, `set_active_workspace`), pane split/close, sidebar state, notification system, monotonic ID generator
- `veil-core::focus` -- `FocusTarget`, `FocusManager`, `KeyRoute`, `route_key_event`
- `veil-core::keyboard` -- `KeyAction` enum (includes `ZoomPane`, `FocusNextPane`, `FocusPreviousPane`), `KeybindingRegistry` with defaults

### Key design decisions

**Layout calculation lives in `veil-core`, not in the renderer.** The layout module produces pure geometry (pixel rects) from the split tree and available dimensions. This keeps it testable without GPU dependencies. The renderer consumes the output.

**Directional navigation is spatial, not tree-structural.** Moving "left" means finding the pane whose rect is to the left of the focused pane, not traversing the tree. This matches user expectations regardless of how the tree was constructed.

**Zoom is a view-layer concept, not a tree mutation.** Zooming doesn't change the `PaneNode` tree. Instead, the layout module checks a zoom flag and returns only the zoomed pane's rect (expanded to full size) when zoom is active. The tree is preserved so unzoom restores the layout exactly.

**Reorder is an index swap on `AppState.workspaces`.** The workspace list is a `Vec<Workspace>` and position determines Cmd+1-9 assignment. Reorder moves a workspace to a new index.

## Implementation Units

### Unit 1: Layout calculation (`veil-core::layout`)

A new module that computes pixel rectangles from the `PaneNode` tree.

**File:** `crates/veil-core/src/layout.rs`

**Types:**

```rust
/// A rectangle in pixel coordinates (origin at top-left of the terminal area).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// X coordinate of the left edge.
    pub x: f32,
    /// Y coordinate of the top edge.
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
}

/// A pane's computed layout: its ID, surface, and pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneLayout {
    /// The pane identifier.
    pub pane_id: PaneId,
    /// The surface this pane renders.
    pub surface_id: SurfaceId,
    /// The computed pixel rectangle.
    pub rect: Rect,
}
```

**Functions:**

```rust
/// Compute pixel rectangles for all panes in a layout tree.
///
/// `available` is the total terminal area (excluding sidebar, chrome, etc.).
/// Returns one `PaneLayout` per leaf node in the tree.
///
/// If `zoomed_pane` is Some, returns a single-element vec with that pane
/// expanded to fill the entire `available` rect. Returns an empty vec if
/// the zoomed pane is not found in the tree.
pub fn compute_layout(
    root: &PaneNode,
    available: Rect,
    zoomed_pane: Option<PaneId>,
) -> Vec<PaneLayout>
```

**Algorithm:**

The layout algorithm is a recursive subdivision of the available rectangle:
- For a `Leaf` node: return a single `PaneLayout` with the given rect.
- For a `Split` node with `Horizontal` direction: split the rect vertically (left/right). First child gets `width * ratio`, second child gets the remainder.
- For a `Split` node with `Vertical` direction: split the rect horizontally (top/bottom). First child gets `height * ratio`, second child gets the remainder.

When `zoomed_pane` is `Some(id)`:
1. Find the leaf with matching `pane_id` in the tree.
2. If found, return `vec![PaneLayout { pane_id: id, surface_id, rect: available }]`.
3. If not found, return `vec![]`.

**Why f32:** Pixel coordinates often involve fractional values when ratios don't divide evenly. Using f32 avoids rounding accumulation during recursive subdivision. The renderer truncates to integer pixels at the final step.

**Test strategy:**

Happy path:
- Single leaf: rect equals available area
- Two-pane horizontal split (0.5 ratio): two rects each half the width, full height
- Two-pane vertical split (0.5 ratio): two rects each half the height, full width
- Three-pane layout (split, then split one child): three rects that tile correctly
- Non-equal ratio (0.3/0.7): rects proportional to ratio
- Deeply nested tree (6 panes): all rects returned, total area covered

Edge cases:
- Zero-width available rect: all rects have zero width
- Zero-height available rect: all rects have zero height
- Ratio at extremes (0.01, 0.99): small pane still gets positive dimensions
- Very small available rect (1x1 pixel): no panics, rects are valid

Zoom:
- Zoomed pane returns single rect equal to available area
- Zoomed pane preserves the correct pane_id and surface_id
- Zoomed pane not found in tree: returns empty vec
- Zoom with None: returns full layout (normal behavior)

Properties (proptest):
- Sum of child widths (horizontal split) equals parent width
- Sum of child heights (vertical split) equals parent height
- Number of `PaneLayout` entries equals leaf count in the tree
- All rects have non-negative dimensions
- No two pane rects overlap (axis-aligned, no shared interior area)

### Unit 2: Directional pane navigation (`veil-core::navigation`)

A new module that resolves directional focus movement (left/right/up/down) based on pane geometry.

**File:** `crates/veil-core/src/navigation.rs`

**Types:**

```rust
/// Cardinal direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
```

**Functions:**

```rust
/// Find the pane in the given direction from the currently focused pane.
///
/// Returns `None` if there is no pane in that direction (focused pane is
/// at the edge of the layout) or if `focused` is not found in `panes`.
///
/// The algorithm finds the nearest pane whose center is in the target
/// direction from the focused pane's center.
pub fn find_pane_in_direction(
    panes: &[PaneLayout],
    focused: PaneId,
    direction: Direction,
) -> Option<PaneId>
```

**Algorithm:**

1. Find the focused pane's rect from the `panes` slice.
2. Compute the focused pane's center point `(cx, cy)`.
3. Filter candidate panes to those whose center is strictly in the target direction:
   - `Left`: candidate center x < focused center x
   - `Right`: candidate center x > focused center x
   - `Up`: candidate center y < focused center y
   - `Down`: candidate center y > focused center y
4. Among candidates, pick the one with the shortest distance to the focused pane's center. Use Euclidean distance. If there's a tie (equidistant candidates), prefer the one closest on the perpendicular axis (i.e., for left/right, prefer the one with closest y; for up/down, prefer the one with closest x).

**Why spatial rather than tree-based:** The user sees panes as rectangles on screen. "Move left" should go to the pane that's visually to the left, regardless of tree structure. A tree-based approach (e.g., sibling of sibling) breaks down with complex layouts where a pane created from a different subtree is visually adjacent.

**Integration with KeyAction:**

The existing `KeyAction` enum has `FocusNextPane` and `FocusPreviousPane` (sequential, bound to Cmd+[/]). For directional navigation, we need new variants:

```rust
// Add to KeyAction enum in keyboard.rs:
/// Focus the pane to the left.
FocusPaneLeft,
/// Focus the pane to the right.
FocusPaneRight,
/// Focus the pane above.
FocusPaneUp,
/// Focus the pane below.
FocusPaneDown,
```

Default bindings in `KeybindingRegistry::with_defaults`:
- `Ctrl+h` -> `FocusPaneLeft`
- `Ctrl+l` -> `FocusPaneRight`
- `Ctrl+k` -> `FocusPaneUp`
- `Ctrl+j` -> `FocusPaneDown`

**Test strategy:**

Happy path:
- Two-pane horizontal split: left pane focused, move right -> right pane; right pane focused, move left -> left pane
- Two-pane vertical split: top pane focused, move down -> bottom pane; bottom focused, move up -> top
- Four-pane grid (2x2): all four directional moves work from center perspective
- Three-pane L-shape: directional navigation resolves to nearest neighbor

Edge cases:
- Single pane: all directions return None
- At left edge, move left: returns None
- At right edge, move right: returns None
- At top edge, move up: returns None
- At bottom edge, move down: returns None
- Focused pane not found in panes slice: returns None
- Empty panes slice: returns None
- Panes with identical centers (degenerate zero-size rects): no panic, returns something deterministic

Complex layouts:
- Asymmetric layout (3 panes: one tall on left, two stacked on right): moving right from left pane picks the one closest on the y-axis
- Deep nesting (6+ panes): navigation still finds the visually nearest pane

### Unit 3: Zoom/unzoom state (`veil-core::workspace` and `veil-core::state`)

Add zoom tracking to the workspace and state layers. This is a thin addition to existing types.

**Changes to `workspace.rs`:**

```rust
// Add field to Workspace struct:
pub struct Workspace {
    // ... existing fields ...
    /// If set, this pane is zoomed (shown fullscreen, layout suppressed).
    pub zoomed_pane: Option<PaneId>,
}
```

`Workspace::new` initializes `zoomed_pane` to `None`.

New methods on `Workspace`:

```rust
/// Toggle zoom on a pane. If the pane is already zoomed, unzoom it.
/// If a different pane is zoomed, switch zoom to the new pane.
/// Returns the new zoom state.
pub fn toggle_zoom(&mut self, pane_id: PaneId) -> Result<Option<PaneId>, WorkspaceError> {
    // Verify pane exists
    if self.layout.find_pane(pane_id).is_none() {
        return Err(WorkspaceError::PaneNotFound(pane_id));
    }
    if self.zoomed_pane == Some(pane_id) {
        self.zoomed_pane = None;
    } else {
        self.zoomed_pane = Some(pane_id);
    }
    Ok(self.zoomed_pane)
}

/// Clear zoom state (e.g., when the zoomed pane is closed).
pub fn clear_zoom(&mut self) {
    self.zoomed_pane = None;
}
```

**Changes to `state.rs`:**

New method on `AppState`:

```rust
/// Toggle zoom on a pane in a workspace.
pub fn toggle_zoom(
    &mut self,
    workspace_id: WorkspaceId,
    pane_id: PaneId,
) -> Result<Option<PaneId>, StateError> {
    Ok(self.workspace_mut(workspace_id)?.toggle_zoom(pane_id)?)
}
```

**Integration with close_pane:**

When a pane is closed via `Workspace::close_pane`, check if the closed pane was zoomed. If so, clear the zoom state. This requires a small modification to `close_pane`:

```rust
// In Workspace::close_pane, after successful removal:
if self.zoomed_pane == Some(pane_id) {
    self.zoomed_pane = None;
}
```

**Test strategy:**

Happy path:
- Toggle zoom on a pane: zoomed_pane becomes Some(pane_id)
- Toggle zoom again on same pane: zoomed_pane becomes None
- Toggle zoom on different pane while zoomed: switches to new pane
- AppState::toggle_zoom delegates correctly

Edge cases:
- Toggle zoom on nonexistent pane: returns PaneNotFound
- Close zoomed pane: zoom clears automatically
- Zoom state preserved across split operations (splitting a non-zoomed pane while another is zoomed)
- New workspace starts with no zoom

### Unit 4: Workspace rename (`veil-core::state`)

Add a rename operation to `AppState`.

**Changes to `state.rs`:**

```rust
/// Rename a workspace.
pub fn rename_workspace(
    &mut self,
    id: WorkspaceId,
    new_name: String,
) -> Result<(), StateError> {
    let ws = self.workspace_mut(id)?;
    ws.name = new_name;
    Ok(())
}
```

New `KeyAction` variant:

```rust
// Add to KeyAction enum in keyboard.rs:
/// Rename the active workspace.
RenameWorkspace,
```

No default keybinding -- rename is triggered from the sidebar context menu or socket API. The `KeyAction` variant exists so it can be bound by the user if desired.

**Validation:** Empty names are allowed (the UI layer can display a fallback like the working directory). Name uniqueness is NOT enforced -- users may have multiple workspaces with the same name (common with "scratch" or default names).

**Test strategy:**

Happy path:
- Rename a workspace: name changes
- Rename preserves all other workspace state (layout, working_directory, branch, zoomed_pane)
- Rename the active workspace: active workspace ID unchanged

Edge cases:
- Rename nonexistent workspace: returns WorkspaceNotFound
- Rename to empty string: succeeds (no validation)
- Rename to same name: succeeds (no-op is fine)

### Unit 5: Workspace reorder (`veil-core::state`)

Add reorder operations to `AppState`. The workspace list position determines Cmd+1-9 assignment and sidebar display order.

**Changes to `state.rs`:**

```rust
/// Move a workspace to a new position in the list.
///
/// `new_index` is clamped to `0..=workspaces.len()-1`. The workspace is
/// removed from its current position and inserted at `new_index`.
pub fn reorder_workspace(
    &mut self,
    id: WorkspaceId,
    new_index: usize,
) -> Result<(), StateError> {
    let current_index = self
        .workspaces
        .iter()
        .position(|ws| ws.id == id)
        .ok_or(StateError::WorkspaceNotFound(id))?;

    let clamped = new_index.min(self.workspaces.len() - 1);
    if current_index != clamped {
        let ws = self.workspaces.remove(current_index);
        self.workspaces.insert(clamped, ws);
    }
    Ok(())
}

/// Swap two workspaces by their IDs.
pub fn swap_workspaces(
    &mut self,
    a: WorkspaceId,
    b: WorkspaceId,
) -> Result<(), StateError> {
    let idx_a = self.workspaces.iter().position(|ws| ws.id == a)
        .ok_or(StateError::WorkspaceNotFound(a))?;
    let idx_b = self.workspaces.iter().position(|ws| ws.id == b)
        .ok_or(StateError::WorkspaceNotFound(b))?;
    self.workspaces.swap(idx_a, idx_b);
    Ok(())
}
```

**Interaction with `set_active_workspace`:** Reorder does not change which workspace is active. The active workspace ID is tracked by `WorkspaceId`, not by index, so reordering is transparent to the active workspace.

**Test strategy:**

Happy path:
- Reorder workspace from position 0 to position 2 (in a 3-workspace list)
- Reorder workspace from position 2 to position 0
- Swap two workspaces: positions exchange
- Active workspace ID unchanged after reorder
- Workspace IDs at each position are correct after reorder

Edge cases:
- Reorder nonexistent workspace: returns WorkspaceNotFound
- Reorder to same position: no-op, succeeds
- Reorder with index beyond list length: clamped to last position
- Reorder in single-workspace list: no-op, succeeds
- Swap nonexistent workspace: returns WorkspaceNotFound
- Swap workspace with itself: no-op, succeeds
- Reorder preserves all workspace internal state (layout, zoom, etc.)

## Acceptance Criteria

1. `cargo build -p veil-core` succeeds
2. `cargo test -p veil-core` passes all tests
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `compute_layout` correctly subdivides pixel rectangles from a `PaneNode` tree and available dimensions
6. `compute_layout` with a zoomed pane returns a single rect filling the available area
7. `find_pane_in_direction` navigates to the spatially nearest pane in each cardinal direction
8. `find_pane_in_direction` returns `None` at layout edges
9. `Ctrl+h/j/k/l` keybindings are registered as defaults for directional focus
10. `toggle_zoom` toggles zoom state on/off for a pane, and switching panes while zoomed works
11. Closing a zoomed pane clears the zoom state
12. `rename_workspace` changes the workspace name
13. `reorder_workspace` moves a workspace to a new list position
14. `swap_workspaces` exchanges two workspaces' positions
15. All operations produce correct errors for invalid inputs (nonexistent pane/workspace IDs)
16. Property-based tests verify layout invariants (area coverage, non-overlap, rect count)

## Dependencies

**No new crate dependencies.** All implementation is pure Rust computation on existing types in `veil-core`. The `proptest` dependency is already available for property-based testing.

**New files:**

| File | Purpose |
|------|---------|
| `crates/veil-core/src/layout.rs` | `Rect`, `PaneLayout`, `compute_layout` |
| `crates/veil-core/src/navigation.rs` | `Direction`, `find_pane_in_direction` |

**Modified files:**

| File | Changes |
|------|---------|
| `crates/veil-core/src/lib.rs` | Add `pub mod layout;` and `pub mod navigation;` |
| `crates/veil-core/src/workspace.rs` | Add `zoomed_pane` field to `Workspace`, `toggle_zoom` and `clear_zoom` methods, zoom cleanup in `close_pane` |
| `crates/veil-core/src/state.rs` | Add `toggle_zoom`, `rename_workspace`, `reorder_workspace`, `swap_workspaces` methods |
| `crates/veil-core/src/keyboard.rs` | Add `FocusPaneLeft/Right/Up/Down` and `RenameWorkspace` variants to `KeyAction`, add Ctrl+hjkl defaults |

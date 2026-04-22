//! Workspace and pane layout types.
//!
//! A workspace contains a tree of panes arranged via splits. Each pane holds
//! a surface ID (opaque handle to a terminal surface managed by veil-ghostty/veil-pty).

use std::fmt;
use std::mem;
use std::path::PathBuf;

/// Unique identifier for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(u64);

impl WorkspaceId {
    /// Create a new `WorkspaceId`.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Unique identifier for a pane within a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u64);

impl PaneId {
    /// Create a new `PaneId`.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PaneId({})", self.0)
    }
}

/// Unique identifier for a terminal surface.
/// Opaque handle — the actual surface is managed by veil-ghostty/veil-pty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(u64);

impl SurfaceId {
    /// Create a new `SurfaceId`.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Direction of a pane split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Side by side.
    Horizontal,
    /// Top and bottom.
    Vertical,
}

/// Tree structure representing pane layout within a workspace.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneNode {
    /// A leaf node containing a single terminal surface.
    Leaf {
        /// The pane identifier.
        pane_id: PaneId,
        /// The surface this pane renders.
        surface_id: SurfaceId,
    },
    /// An interior node splitting two children.
    Split {
        /// Direction of the split.
        direction: SplitDirection,
        /// Fraction of space allocated to the first child (0.0..=1.0).
        ratio: f32,
        /// First child.
        first: Box<PaneNode>,
        /// Second child.
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Collect all pane IDs in the tree.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            PaneNode::Leaf { pane_id, .. } => vec![*pane_id],
            PaneNode::Split { first, second, .. } => {
                let mut ids = first.pane_ids();
                ids.extend(second.pane_ids());
                ids
            }
        }
    }

    /// Collect all surface IDs in the tree.
    pub fn surface_ids(&self) -> Vec<SurfaceId> {
        match self {
            PaneNode::Leaf { surface_id, .. } => vec![*surface_id],
            PaneNode::Split { first, second, .. } => {
                let mut ids = first.surface_ids();
                ids.extend(second.surface_ids());
                ids
            }
        }
    }

    /// Locate a pane by ID.
    pub fn find_pane(&self, id: PaneId) -> Option<&PaneNode> {
        match self {
            PaneNode::Leaf { pane_id, .. } => {
                if *pane_id == id {
                    Some(self)
                } else {
                    None
                }
            }
            PaneNode::Split { first, second, .. } => {
                first.find_pane(id).or_else(|| second.find_pane(id))
            }
        }
    }

    /// Count leaf nodes.
    pub fn pane_count(&self) -> usize {
        match self {
            PaneNode::Leaf { .. } => 1,
            PaneNode::Split { first, second, .. } => first.pane_count() + second.pane_count(),
        }
    }
}

/// Errors that can occur during workspace operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// The specified pane was not found.
    #[error("pane {0} not found")]
    PaneNotFound(PaneId),
    /// Cannot close the last pane in a workspace.
    #[error("cannot close the last pane in workspace")]
    LastPane,
}

/// A workspace: a named collection of panes with a layout tree.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Unique identifier.
    pub id: WorkspaceId,
    /// Display name.
    pub name: String,
    /// Working directory.
    pub working_directory: PathBuf,
    /// Pane layout tree.
    pub layout: PaneNode,
    /// Git branch if applicable (detected, not managed by Veil).
    pub branch: Option<String>,
    /// If set, this pane is zoomed (shown fullscreen, layout suppressed).
    pub zoomed_pane: Option<PaneId>,
}

impl Workspace {
    /// Create a workspace with a single pane.
    pub fn new(
        id: WorkspaceId,
        name: String,
        working_directory: PathBuf,
        initial_pane_id: PaneId,
        initial_surface_id: SurfaceId,
    ) -> Self {
        Self {
            id,
            name,
            working_directory,
            layout: PaneNode::Leaf { pane_id: initial_pane_id, surface_id: initial_surface_id },
            branch: None,
            zoomed_pane: None,
        }
    }

    /// Split an existing pane.
    pub fn split_pane(
        &mut self,
        pane_id: PaneId,
        direction: SplitDirection,
        new_pane_id: PaneId,
        new_surface_id: SurfaceId,
    ) -> Result<(), WorkspaceError> {
        if !Self::split_node(&mut self.layout, pane_id, direction, new_pane_id, new_surface_id) {
            return Err(WorkspaceError::PaneNotFound(pane_id));
        }
        Ok(())
    }

    /// Recursively find the target leaf and replace it with a split node.
    /// Returns true if the pane was found and split.
    fn split_node(
        node: &mut PaneNode,
        target: PaneId,
        direction: SplitDirection,
        new_pane_id: PaneId,
        new_surface_id: SurfaceId,
    ) -> bool {
        match node {
            PaneNode::Leaf { pane_id, .. } => {
                if *pane_id == target {
                    // Use a sentinel leaf to move the old node out without cloning.
                    let sentinel =
                        PaneNode::Leaf { pane_id: PaneId::new(0), surface_id: SurfaceId::new(0) };
                    let old_leaf = mem::replace(node, sentinel);
                    let new_leaf =
                        PaneNode::Leaf { pane_id: new_pane_id, surface_id: new_surface_id };
                    *node = PaneNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(old_leaf),
                        second: Box::new(new_leaf),
                    };
                    true
                } else {
                    false
                }
            }
            PaneNode::Split { first, second, .. } => {
                Self::split_node(first, target, direction, new_pane_id, new_surface_id)
                    || Self::split_node(second, target, direction, new_pane_id, new_surface_id)
            }
        }
    }

    /// Remove a pane. Returns the closed surface ID.
    /// If it was the last pane, returns `LastPane` error.
    pub fn close_pane(&mut self, pane_id: PaneId) -> Result<Option<SurfaceId>, WorkspaceError> {
        // If the layout is a single leaf, we can't close it.
        if let PaneNode::Leaf { pane_id: leaf_id, .. } = &self.layout {
            if *leaf_id == pane_id {
                return Err(WorkspaceError::LastPane);
            }
            return Err(WorkspaceError::PaneNotFound(pane_id));
        }

        // Single traversal: find and remove the pane, promoting its sibling.
        let surface_id = Self::remove_pane(&mut self.layout, pane_id)
            .ok_or(WorkspaceError::PaneNotFound(pane_id))?;

        if self.zoomed_pane == Some(pane_id) {
            self.zoomed_pane = None;
        }

        Ok(Some(surface_id))
    }

    /// Recursively remove a pane from the tree, promoting its sibling.
    /// Returns the surface ID of the removed pane if found.
    fn remove_pane(node: &mut PaneNode, target: PaneId) -> Option<SurfaceId> {
        let PaneNode::Split { first, second, .. } = node else {
            return None;
        };

        // Check if the target is a direct child — if so, promote the sibling.
        if let PaneNode::Leaf { pane_id, surface_id } = first.as_ref() {
            if *pane_id == target {
                let closed_surface = *surface_id;
                // Move the sibling into this node's slot without cloning.
                let sentinel =
                    PaneNode::Leaf { pane_id: PaneId::new(0), surface_id: SurfaceId::new(0) };
                let sibling = mem::replace(second.as_mut(), sentinel);
                *node = sibling;
                return Some(closed_surface);
            }
        }
        if let PaneNode::Leaf { pane_id, surface_id } = second.as_ref() {
            if *pane_id == target {
                let closed_surface = *surface_id;
                let sentinel =
                    PaneNode::Leaf { pane_id: PaneId::new(0), surface_id: SurfaceId::new(0) };
                let sibling = mem::replace(first.as_mut(), sentinel);
                *node = sibling;
                return Some(closed_surface);
            }
        }

        // Recurse into children.
        Self::remove_pane(first, target).or_else(|| Self::remove_pane(second, target))
    }

    /// Get all pane IDs in this workspace.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.layout.pane_ids()
    }

    /// Toggle zoom on a pane. If the pane is already zoomed, unzoom it.
    /// If a different pane is zoomed, switch zoom to the new pane.
    /// Returns the new zoom state.
    pub fn toggle_zoom(&mut self, pane_id: PaneId) -> Result<Option<PaneId>, WorkspaceError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_workspace() -> Workspace {
        Workspace::new(
            WorkspaceId::new(1),
            "test".to_string(),
            PathBuf::from("/tmp/test"),
            PaneId::new(1),
            SurfaceId::new(1),
        )
    }

    // --- Workspace::new ---

    #[test]
    fn new_workspace_has_one_pane() {
        let ws = make_workspace();
        assert_eq!(ws.layout.pane_count(), 1);
    }

    // --- split_pane ---

    #[test]
    fn split_pane_horizontal_increases_count() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        assert_eq!(ws.layout.pane_count(), 2);
        let sids = ws.layout.surface_ids();
        assert!(sids.contains(&SurfaceId::new(1)));
        assert!(sids.contains(&SurfaceId::new(2)));
    }

    #[test]
    fn split_pane_vertical_creates_split_node() {
        let mut ws = make_workspace();
        ws.split_pane(PaneId::new(1), SplitDirection::Vertical, PaneId::new(2), SurfaceId::new(2))
            .expect("split should succeed");
        assert_eq!(ws.layout.pane_count(), 2);
        match &ws.layout {
            PaneNode::Split { direction, .. } => {
                assert_eq!(*direction, SplitDirection::Vertical);
            }
            PaneNode::Leaf { .. } => panic!("expected split node after vertical split"),
        }
    }

    #[test]
    fn split_nonexistent_pane_returns_error() {
        let mut ws = make_workspace();
        let result = ws.split_pane(
            PaneId::new(999),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            WorkspaceError::PaneNotFound(id) => assert_eq!(id, PaneId::new(999)),
            other @ WorkspaceError::LastPane => panic!("expected PaneNotFound, got {other:?}"),
        }
    }

    // --- close_pane ---

    #[test]
    fn close_pane_in_two_pane_workspace() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        let closed = ws.close_pane(PaneId::new(2)).expect("close should succeed");
        assert_eq!(closed, Some(SurfaceId::new(2)));
        assert_eq!(ws.layout.pane_count(), 1);
    }

    #[test]
    fn close_last_pane_returns_error() {
        let mut ws = make_workspace();
        let result = ws.close_pane(PaneId::new(1));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorkspaceError::LastPane));
    }

    // --- PaneNode::pane_ids ---

    #[test]
    fn pane_ids_returns_all_ids_in_nested_tree() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        ws.split_pane(PaneId::new(2), SplitDirection::Vertical, PaneId::new(3), SurfaceId::new(3))
            .expect("split should succeed");
        let ids = ws.pane_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&PaneId::new(1)));
        assert!(ids.contains(&PaneId::new(2)));
        assert!(ids.contains(&PaneId::new(3)));
    }

    // --- PaneNode::surface_ids ---

    #[test]
    fn surface_ids_returns_all_surface_ids() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        let sids = ws.layout.surface_ids();
        assert_eq!(sids.len(), 2);
        assert!(sids.contains(&SurfaceId::new(1)));
        assert!(sids.contains(&SurfaceId::new(2)));
    }

    // --- PaneNode::find_pane ---

    #[test]
    fn find_pane_locates_leaf_in_nested_tree() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        ws.split_pane(PaneId::new(2), SplitDirection::Vertical, PaneId::new(3), SurfaceId::new(3))
            .expect("split should succeed");
        let found = ws.layout.find_pane(PaneId::new(3));
        assert!(found.is_some());
        match found.unwrap() {
            PaneNode::Leaf { pane_id, .. } => assert_eq!(*pane_id, PaneId::new(3)),
            PaneNode::Split { .. } => panic!("expected leaf node"),
        }
    }

    #[test]
    fn find_pane_returns_none_for_nonexistent() {
        let ws = make_workspace();
        assert!(ws.layout.find_pane(PaneId::new(999)).is_none());
    }

    // --- PaneNode::pane_count ---

    #[test]
    fn pane_count_single_leaf() {
        let node = PaneNode::Leaf { pane_id: PaneId::new(1), surface_id: SurfaceId::new(1) };
        assert_eq!(node.pane_count(), 1);
    }

    // --- Split ratio validation ---

    #[test]
    fn split_ratio_default_is_valid() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        match &ws.layout {
            PaneNode::Split { ratio, .. } => {
                assert!(*ratio > 0.0, "ratio must be > 0.0");
                assert!(*ratio < 1.0, "ratio must be < 1.0");
            }
            PaneNode::Leaf { .. } => panic!("expected split node"),
        }
    }

    // --- VEI-11: Zoom/unzoom state ---

    #[test]
    fn new_workspace_has_no_zoom() {
        let ws = make_workspace();
        assert_eq!(ws.zoomed_pane, None);
    }

    #[test]
    fn toggle_zoom_on_pane_sets_zoomed() {
        let mut ws = make_workspace();
        let result = ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        assert_eq!(result, Some(PaneId::new(1)));
        assert_eq!(ws.zoomed_pane, Some(PaneId::new(1)));
    }

    #[test]
    fn toggle_zoom_again_on_same_pane_unzooms() {
        let mut ws = make_workspace();
        ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        let result = ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        assert_eq!(result, None);
        assert_eq!(ws.zoomed_pane, None);
    }

    #[test]
    fn toggle_zoom_on_different_pane_switches_zoom() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        let result = ws.toggle_zoom(PaneId::new(2)).expect("toggle_zoom should succeed");
        assert_eq!(result, Some(PaneId::new(2)));
        assert_eq!(ws.zoomed_pane, Some(PaneId::new(2)));
    }

    #[test]
    fn toggle_zoom_on_nonexistent_pane_returns_error() {
        let mut ws = make_workspace();
        let result = ws.toggle_zoom(PaneId::new(999));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorkspaceError::PaneNotFound(_)));
    }

    #[test]
    fn close_zoomed_pane_clears_zoom() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        ws.toggle_zoom(PaneId::new(2)).expect("toggle_zoom should succeed");
        assert_eq!(ws.zoomed_pane, Some(PaneId::new(2)));
        ws.close_pane(PaneId::new(2)).expect("close should succeed");
        assert_eq!(ws.zoomed_pane, None, "zoom should be cleared when zoomed pane is closed");
    }

    #[test]
    fn zoom_preserved_across_split_of_different_pane() {
        let mut ws = make_workspace();
        ws.split_pane(
            PaneId::new(1),
            SplitDirection::Horizontal,
            PaneId::new(2),
            SurfaceId::new(2),
        )
        .expect("split should succeed");
        ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        // Split the non-zoomed pane
        ws.split_pane(PaneId::new(2), SplitDirection::Vertical, PaneId::new(3), SurfaceId::new(3))
            .expect("split should succeed");
        assert_eq!(
            ws.zoomed_pane,
            Some(PaneId::new(1)),
            "zoom should be preserved when splitting a different pane"
        );
    }

    #[test]
    fn clear_zoom_resets_state() {
        let mut ws = make_workspace();
        ws.toggle_zoom(PaneId::new(1)).expect("toggle_zoom should succeed");
        ws.clear_zoom();
        assert_eq!(ws.zoomed_pane, None);
    }

    // --- Deep nesting ---

    #[test]
    fn deep_nesting_preserves_tree_integrity() {
        let mut ws = make_workspace();
        // Build a tree with 6 panes via 5 splits
        for i in 2..=6 {
            // Always split the previously added pane
            ws.split_pane(
                PaneId::new(i - 1),
                if i % 2 == 0 { SplitDirection::Horizontal } else { SplitDirection::Vertical },
                PaneId::new(i),
                SurfaceId::new(i),
            )
            .expect("split should succeed");
        }
        assert_eq!(ws.layout.pane_count(), 6);
        let ids = ws.pane_ids();
        assert_eq!(ids.len(), 6);
        for i in 1..=6 {
            assert!(ids.contains(&PaneId::new(i)));
        }
    }
}

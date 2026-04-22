//! Workspace and pane layout types.
//!
//! A workspace contains a tree of panes arranged via splits. Each pane holds
//! a surface ID (opaque handle to a terminal surface managed by veil-ghostty/veil-pty).

use std::fmt;
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
        todo!()
    }

    /// Collect all surface IDs in the tree.
    pub fn surface_ids(&self) -> Vec<SurfaceId> {
        todo!()
    }

    /// Locate a pane by ID.
    pub fn find_pane(&self, _id: PaneId) -> Option<&PaneNode> {
        todo!()
    }

    /// Count leaf nodes.
    pub fn pane_count(&self) -> usize {
        todo!()
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
        }
    }

    /// Split an existing pane.
    pub fn split_pane(
        &mut self,
        _pane_id: PaneId,
        _direction: SplitDirection,
        _new_pane_id: PaneId,
        _new_surface_id: SurfaceId,
    ) -> Result<(), WorkspaceError> {
        todo!()
    }

    /// Remove a pane. Returns the closed surface ID.
    /// If it was the last pane, returns `LastPane` error.
    pub fn close_pane(&mut self, _pane_id: PaneId) -> Result<Option<SurfaceId>, WorkspaceError> {
        todo!()
    }

    /// Get all pane IDs in this workspace.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.layout.pane_ids()
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
            _ => panic!("expected split node after vertical split"),
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
            other => panic!("expected PaneNotFound, got {other:?}"),
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
            _ => panic!("expected leaf node"),
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
            _ => panic!("expected split node"),
        }
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

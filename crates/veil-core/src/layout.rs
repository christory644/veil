//! Layout calculation -- translating the abstract `PaneNode` tree into
//! concrete pixel rectangles for the renderer.

use crate::workspace::{PaneId, PaneNode, SurfaceId};

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

/// Compute pixel rectangles for all panes in a layout tree.
///
/// `available` is the total terminal area (excluding sidebar, chrome, etc.).
/// Returns one `PaneLayout` per leaf node in the tree.
///
/// If `zoomed_pane` is Some, returns a single-element vec with that pane
/// expanded to fill the entire `available` rect. Returns an empty vec if
/// the zoomed pane is not found in the tree.
pub fn compute_layout(
    _root: &PaneNode,
    _available: Rect,
    _zoomed_pane: Option<PaneId>,
) -> Vec<PaneLayout> {
    // STUB: returns empty vec -- tests will fail until implemented.
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{PaneId, PaneNode, SplitDirection, SurfaceId};

    // --- Helpers ---

    fn leaf(pane_id: u64, surface_id: u64) -> PaneNode {
        PaneNode::Leaf { pane_id: PaneId::new(pane_id), surface_id: SurfaceId::new(surface_id) }
    }

    fn split_h(ratio: f32, first: PaneNode, second: PaneNode) -> PaneNode {
        PaneNode::Split {
            direction: SplitDirection::Horizontal,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn split_v(ratio: f32, first: PaneNode, second: PaneNode) -> PaneNode {
        PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn full_rect() -> Rect {
        Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 }
    }

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn find_layout(layouts: &[PaneLayout], pane_id: u64) -> &PaneLayout {
        layouts
            .iter()
            .find(|l| l.pane_id == PaneId::new(pane_id))
            .unwrap_or_else(|| panic!("pane {pane_id} not found in layouts"))
    }

    // ============================================================
    // Happy path: single leaf
    // ============================================================

    #[test]
    fn single_leaf_fills_available_area() {
        let root = leaf(1, 1);
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 1);
        let l = &layouts[0];
        assert_eq!(l.pane_id, PaneId::new(1));
        assert_eq!(l.surface_id, SurfaceId::new(1));
        assert!(approx_eq(l.rect.x, 0.0));
        assert!(approx_eq(l.rect.y, 0.0));
        assert!(approx_eq(l.rect.width, 800.0));
        assert!(approx_eq(l.rect.height, 600.0));
    }

    // ============================================================
    // Happy path: horizontal split (side by side)
    // ============================================================

    #[test]
    fn horizontal_split_equal_ratio() {
        let root = split_h(0.5, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);

        let left = find_layout(&layouts, 1);
        let right = find_layout(&layouts, 2);

        // Left pane: x=0, width=400
        assert!(approx_eq(left.rect.x, 0.0));
        assert!(approx_eq(left.rect.width, 400.0));
        assert!(approx_eq(left.rect.height, 600.0));

        // Right pane: x=400, width=400
        assert!(approx_eq(right.rect.x, 400.0));
        assert!(approx_eq(right.rect.width, 400.0));
        assert!(approx_eq(right.rect.height, 600.0));
    }

    // ============================================================
    // Happy path: vertical split (top/bottom)
    // ============================================================

    #[test]
    fn vertical_split_equal_ratio() {
        let root = split_v(0.5, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);

        let top = find_layout(&layouts, 1);
        let bottom = find_layout(&layouts, 2);

        // Top pane: y=0, height=300
        assert!(approx_eq(top.rect.y, 0.0));
        assert!(approx_eq(top.rect.height, 300.0));
        assert!(approx_eq(top.rect.width, 800.0));

        // Bottom pane: y=300, height=300
        assert!(approx_eq(bottom.rect.y, 300.0));
        assert!(approx_eq(bottom.rect.height, 300.0));
        assert!(approx_eq(bottom.rect.width, 800.0));
    }

    // ============================================================
    // Happy path: three-pane layout
    // ============================================================

    #[test]
    fn three_pane_layout_tiles_correctly() {
        // Left pane | (top-right / bottom-right)
        let right_split = split_v(0.5, leaf(2, 2), leaf(3, 3));
        let root = split_h(0.5, leaf(1, 1), right_split);
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 3);

        let left = find_layout(&layouts, 1);
        let top_right = find_layout(&layouts, 2);
        let bottom_right = find_layout(&layouts, 3);

        // Left pane: full height, half width
        assert!(approx_eq(left.rect.x, 0.0));
        assert!(approx_eq(left.rect.width, 400.0));
        assert!(approx_eq(left.rect.height, 600.0));

        // Top-right: x=400, half height
        assert!(approx_eq(top_right.rect.x, 400.0));
        assert!(approx_eq(top_right.rect.y, 0.0));
        assert!(approx_eq(top_right.rect.width, 400.0));
        assert!(approx_eq(top_right.rect.height, 300.0));

        // Bottom-right: x=400, y=300, half height
        assert!(approx_eq(bottom_right.rect.x, 400.0));
        assert!(approx_eq(bottom_right.rect.y, 300.0));
        assert!(approx_eq(bottom_right.rect.width, 400.0));
        assert!(approx_eq(bottom_right.rect.height, 300.0));
    }

    // ============================================================
    // Happy path: non-equal ratio
    // ============================================================

    #[test]
    fn non_equal_ratio_horizontal() {
        let root = split_h(0.3, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);

        let left = find_layout(&layouts, 1);
        let right = find_layout(&layouts, 2);

        assert!(approx_eq(left.rect.width, 240.0)); // 800 * 0.3
        assert!(approx_eq(right.rect.width, 560.0)); // 800 * 0.7
        assert!(approx_eq(right.rect.x, 240.0));
    }

    // ============================================================
    // Happy path: deeply nested tree (6 panes)
    // ============================================================

    #[test]
    fn deeply_nested_six_panes_all_returned() {
        // Build a chain: each split adds one new pane
        let node = split_v(
            0.5,
            split_h(
                0.5,
                split_v(0.5, leaf(1, 1), leaf(2, 2)),
                split_v(0.5, leaf(3, 3), leaf(4, 4)),
            ),
            split_h(0.5, leaf(5, 5), leaf(6, 6)),
        );
        let layouts = compute_layout(&node, full_rect(), None);
        assert_eq!(layouts.len(), 6);

        // All pane IDs present
        for i in 1..=6 {
            assert!(
                layouts.iter().any(|l| l.pane_id == PaneId::new(i)),
                "pane {i} should be present in layouts"
            );
        }

        // Total area should approximately equal available area
        let total_area: f32 = layouts.iter().map(|l| l.rect.width * l.rect.height).sum();
        assert!(
            approx_eq(total_area, 800.0 * 600.0),
            "total area {total_area} should equal available area {}",
            800.0 * 600.0
        );
    }

    // ============================================================
    // Edge case: zero-width available rect
    // ============================================================

    #[test]
    fn zero_width_available_rect() {
        let root = split_h(0.5, leaf(1, 1), leaf(2, 2));
        let available = Rect { x: 0.0, y: 0.0, width: 0.0, height: 600.0 };
        let layouts = compute_layout(&root, available, None);
        assert_eq!(layouts.len(), 2);
        for l in &layouts {
            assert!(approx_eq(l.rect.width, 0.0));
        }
    }

    // ============================================================
    // Edge case: zero-height available rect
    // ============================================================

    #[test]
    fn zero_height_available_rect() {
        let root = split_v(0.5, leaf(1, 1), leaf(2, 2));
        let available = Rect { x: 0.0, y: 0.0, width: 800.0, height: 0.0 };
        let layouts = compute_layout(&root, available, None);
        assert_eq!(layouts.len(), 2);
        for l in &layouts {
            assert!(approx_eq(l.rect.height, 0.0));
        }
    }

    // ============================================================
    // Edge case: extreme ratios
    // ============================================================

    #[test]
    fn extreme_ratio_small_first_child() {
        let root = split_h(0.01, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);

        let left = find_layout(&layouts, 1);
        let right = find_layout(&layouts, 2);

        assert!(left.rect.width > 0.0, "even tiny ratio should give positive width");
        assert!(approx_eq(left.rect.width, 8.0)); // 800 * 0.01
        assert!(approx_eq(right.rect.width, 792.0)); // 800 * 0.99
    }

    #[test]
    fn extreme_ratio_large_first_child() {
        let root = split_h(0.99, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);

        let right = find_layout(&layouts, 2);
        assert!(right.rect.width > 0.0, "even tiny remainder should give positive width");
    }

    // ============================================================
    // Edge case: very small available rect (1x1)
    // ============================================================

    #[test]
    fn very_small_rect_no_panics() {
        let root = split_h(0.5, split_v(0.5, leaf(1, 1), leaf(2, 2)), leaf(3, 3));
        let available = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
        let layouts = compute_layout(&root, available, None);
        assert_eq!(layouts.len(), 3);
        for l in &layouts {
            assert!(l.rect.width >= 0.0);
            assert!(l.rect.height >= 0.0);
        }
    }

    // ============================================================
    // Zoom: pane found, returns single rect filling available
    // ============================================================

    #[test]
    fn zoom_returns_single_rect_filling_available() {
        let root = split_h(0.5, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), Some(PaneId::new(1)));
        assert_eq!(layouts.len(), 1);
        let l = &layouts[0];
        assert_eq!(l.pane_id, PaneId::new(1));
        assert!(approx_eq(l.rect.x, 0.0));
        assert!(approx_eq(l.rect.y, 0.0));
        assert!(approx_eq(l.rect.width, 800.0));
        assert!(approx_eq(l.rect.height, 600.0));
    }

    // ============================================================
    // Zoom: preserves correct pane_id and surface_id
    // ============================================================

    #[test]
    fn zoom_preserves_pane_and_surface_ids() {
        let root = split_h(0.5, leaf(1, 10), leaf(2, 20));
        let layouts = compute_layout(&root, full_rect(), Some(PaneId::new(2)));
        assert_eq!(layouts.len(), 1);
        assert_eq!(layouts[0].pane_id, PaneId::new(2));
        assert_eq!(layouts[0].surface_id, SurfaceId::new(20));
    }

    // ============================================================
    // Zoom: pane not found returns empty vec
    // ============================================================

    #[test]
    fn zoom_pane_not_found_returns_empty() {
        let root = split_h(0.5, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), Some(PaneId::new(999)));
        assert!(layouts.is_empty());
    }

    // ============================================================
    // Zoom: None means normal layout
    // ============================================================

    #[test]
    fn zoom_none_returns_full_layout() {
        let root = split_h(0.5, leaf(1, 1), leaf(2, 2));
        let layouts = compute_layout(&root, full_rect(), None);
        assert_eq!(layouts.len(), 2);
    }

    // ============================================================
    // Property tests (proptest)
    // ============================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn arb_ratio() -> impl Strategy<Value = f32> {
            (1u16..99).prop_map(|n| f32::from(n) / 100.0)
        }

        fn arb_direction() -> impl Strategy<Value = SplitDirection> {
            prop_oneof![Just(SplitDirection::Horizontal), Just(SplitDirection::Vertical),]
        }

        // Generate a PaneNode tree with a bounded number of leaves.
        fn arb_pane_tree(max_depth: u32) -> BoxedStrategy<(PaneNode, usize)> {
            // Leaf case
            let leaf_strategy = (1u64..1000)
                .prop_map(|id| {
                    (
                        PaneNode::Leaf {
                            pane_id: PaneId::new(id),
                            surface_id: SurfaceId::new(id + 10000),
                        },
                        1usize,
                    )
                })
                .boxed();

            if max_depth == 0 {
                return leaf_strategy;
            }

            let split_strategy = (
                arb_direction(),
                arb_ratio(),
                arb_pane_tree(max_depth - 1),
                arb_pane_tree(max_depth - 1),
            )
                .prop_map(|(dir, ratio, (first, c1), (second, c2))| {
                    (
                        PaneNode::Split {
                            direction: dir,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second),
                        },
                        c1 + c2,
                    )
                })
                .boxed();

            prop_oneof![leaf_strategy, split_strategy].boxed()
        }

        fn arb_positive_dimensions() -> impl Strategy<Value = (f32, f32)> {
            (1.0f32..2000.0, 1.0f32..2000.0)
        }

        proptest! {
            /// The number of PaneLayout entries equals the leaf count.
            #[test]
            fn layout_count_equals_leaf_count(
                (tree, leaf_count) in arb_pane_tree(3),
                (w, h) in arb_positive_dimensions(),
            ) {
                let available = Rect { x: 0.0, y: 0.0, width: w, height: h };
                let layouts = compute_layout(&tree, available, None);
                prop_assert_eq!(layouts.len(), leaf_count);
            }

            /// All rects have non-negative dimensions.
            #[test]
            fn all_rects_non_negative(
                (tree, _) in arb_pane_tree(3),
                (w, h) in arb_positive_dimensions(),
            ) {
                let available = Rect { x: 0.0, y: 0.0, width: w, height: h };
                let layouts = compute_layout(&tree, available, None);
                for l in &layouts {
                    prop_assert!(l.rect.width >= 0.0, "width {} < 0", l.rect.width);
                    prop_assert!(l.rect.height >= 0.0, "height {} < 0", l.rect.height);
                }
            }

            /// Total area of all pane rects approximately equals available area.
            #[test]
            fn total_area_equals_available(
                (tree, _) in arb_pane_tree(3),
                (w, h) in arb_positive_dimensions(),
            ) {
                let available = Rect { x: 0.0, y: 0.0, width: w, height: h };
                let layouts = compute_layout(&tree, available, None);
                let total_area: f32 = layouts.iter().map(|l| l.rect.width * l.rect.height).sum();
                let expected_area = w * h;
                // Allow small floating-point tolerance
                let tolerance = expected_area * 0.001 + 0.1;
                prop_assert!(
                    (total_area - expected_area).abs() < tolerance,
                    "total area {} != expected {} (tolerance {})",
                    total_area, expected_area, tolerance
                );
            }

            /// No two pane rects have overlapping interior area.
            #[test]
            fn no_rects_overlap(
                (tree, _) in arb_pane_tree(3),
                (w, h) in arb_positive_dimensions(),
            ) {
                let available = Rect { x: 0.0, y: 0.0, width: w, height: h };
                let layouts = compute_layout(&tree, available, None);
                for i in 0..layouts.len() {
                    for j in (i + 1)..layouts.len() {
                        let a = &layouts[i].rect;
                        let b = &layouts[j].rect;
                        // Two rects overlap if their projections overlap on both axes
                        let x_overlap = a.x < (b.x + b.width) && b.x < (a.x + a.width);
                        let y_overlap = a.y < (b.y + b.height) && b.y < (a.y + a.height);
                        if x_overlap && y_overlap {
                            // Calculate overlap area
                            let ox = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                            let oy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                            let overlap_area = ox.max(0.0) * oy.max(0.0);
                            prop_assert!(
                                overlap_area < 0.01,
                                "rects {} and {} overlap by area {}",
                                i, j, overlap_area
                            );
                        }
                    }
                }
            }
        }
    }
}

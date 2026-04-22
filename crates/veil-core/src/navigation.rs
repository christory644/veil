//! Directional pane navigation -- resolves spatial focus movement
//! (left/right/up/down) based on pane geometry.

use crate::layout::PaneLayout;
use crate::workspace::PaneId;

/// Cardinal direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Focus the pane to the left.
    Left,
    /// Focus the pane to the right.
    Right,
    /// Focus the pane above.
    Up,
    /// Focus the pane below.
    Down,
}

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
) -> Option<PaneId> {
    // 1. Find the focused pane's rect.
    let focused_layout = panes.iter().find(|p| p.pane_id == focused)?;
    let fr = &focused_layout.rect;
    let cx = fr.x + fr.width / 2.0;
    let cy = fr.y + fr.height / 2.0;

    // 2. Filter candidates whose center is strictly in the target direction,
    //    then pick the nearest by Euclidean distance (tie-break on perpendicular axis).
    panes
        .iter()
        .filter(|p| p.pane_id != focused)
        .filter_map(|p| {
            let r = &p.rect;
            let px = r.x + r.width / 2.0;
            let py = r.y + r.height / 2.0;

            let in_direction = match direction {
                Direction::Left => px < cx,
                Direction::Right => px > cx,
                Direction::Up => py < cy,
                Direction::Down => py > cy,
            };

            if in_direction {
                let dx = px - cx;
                let dy = py - cy;
                let dist_sq = dx * dx + dy * dy;
                // Perpendicular distance for tie-breaking:
                // left/right -> prefer closest y; up/down -> prefer closest x
                let perp = match direction {
                    Direction::Left | Direction::Right => (py - cy).abs(),
                    Direction::Up | Direction::Down => (px - cx).abs(),
                };
                Some((p.pane_id, dist_sq, perp))
            } else {
                None
            }
        })
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|(id, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{PaneLayout, Rect};
    use crate::workspace::{PaneId, SurfaceId};

    // --- Helpers ---

    fn make_pane(pane_id: u64, x: f32, y: f32, width: f32, height: f32) -> PaneLayout {
        PaneLayout {
            pane_id: PaneId::new(pane_id),
            surface_id: SurfaceId::new(pane_id),
            rect: Rect { x, y, width, height },
        }
    }

    /// Two-pane horizontal split: left (1) | right (2)
    fn two_pane_horizontal() -> Vec<PaneLayout> {
        vec![make_pane(1, 0.0, 0.0, 400.0, 600.0), make_pane(2, 400.0, 0.0, 400.0, 600.0)]
    }

    /// Two-pane vertical split: top (1) / bottom (2)
    fn two_pane_vertical() -> Vec<PaneLayout> {
        vec![make_pane(1, 0.0, 0.0, 800.0, 300.0), make_pane(2, 0.0, 300.0, 800.0, 300.0)]
    }

    /// Four-pane grid (2x2):
    /// +---------+---------+
    /// |  1 (TL) |  2 (TR) |
    /// +---------+---------+
    /// |  3 (BL) |  4 (BR) |
    /// +---------+---------+
    fn four_pane_grid() -> Vec<PaneLayout> {
        vec![
            make_pane(1, 0.0, 0.0, 400.0, 300.0),
            make_pane(2, 400.0, 0.0, 400.0, 300.0),
            make_pane(3, 0.0, 300.0, 400.0, 300.0),
            make_pane(4, 400.0, 300.0, 400.0, 300.0),
        ]
    }

    /// Three-pane L-shape: one tall left (1), two stacked right (2, 3)
    /// +-------+-------+
    /// |       |  2    |
    /// |   1   +-------+
    /// |       |  3    |
    /// +-------+-------+
    fn three_pane_asymmetric() -> Vec<PaneLayout> {
        vec![
            make_pane(1, 0.0, 0.0, 400.0, 600.0),
            make_pane(2, 400.0, 0.0, 400.0, 300.0),
            make_pane(3, 400.0, 300.0, 400.0, 300.0),
        ]
    }

    // ============================================================
    // Happy path: two-pane horizontal
    // ============================================================

    #[test]
    fn horizontal_move_right_from_left() {
        let panes = two_pane_horizontal();
        let result = find_pane_in_direction(&panes, PaneId::new(1), Direction::Right);
        assert_eq!(result, Some(PaneId::new(2)));
    }

    #[test]
    fn horizontal_move_left_from_right() {
        let panes = two_pane_horizontal();
        let result = find_pane_in_direction(&panes, PaneId::new(2), Direction::Left);
        assert_eq!(result, Some(PaneId::new(1)));
    }

    // ============================================================
    // Happy path: two-pane vertical
    // ============================================================

    #[test]
    fn vertical_move_down_from_top() {
        let panes = two_pane_vertical();
        let result = find_pane_in_direction(&panes, PaneId::new(1), Direction::Down);
        assert_eq!(result, Some(PaneId::new(2)));
    }

    #[test]
    fn vertical_move_up_from_bottom() {
        let panes = two_pane_vertical();
        let result = find_pane_in_direction(&panes, PaneId::new(2), Direction::Up);
        assert_eq!(result, Some(PaneId::new(1)));
    }

    // ============================================================
    // Happy path: four-pane grid (all directions from each)
    // ============================================================

    #[test]
    fn grid_top_left_move_right() {
        let panes = four_pane_grid();
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(1), Direction::Right),
            Some(PaneId::new(2))
        );
    }

    #[test]
    fn grid_top_left_move_down() {
        let panes = four_pane_grid();
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(1), Direction::Down),
            Some(PaneId::new(3))
        );
    }

    #[test]
    fn grid_bottom_right_move_left() {
        let panes = four_pane_grid();
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(4), Direction::Left),
            Some(PaneId::new(3))
        );
    }

    #[test]
    fn grid_bottom_right_move_up() {
        let panes = four_pane_grid();
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(4), Direction::Up),
            Some(PaneId::new(2))
        );
    }

    // ============================================================
    // Happy path: asymmetric three-pane layout
    // ============================================================

    #[test]
    fn asymmetric_move_right_from_left_picks_nearest_y() {
        // Pane 1 center is at (200, 300). Panes 2 and 3 are to the right.
        // Pane 2 center at (600, 150), Pane 3 center at (600, 450).
        // Both are equidistant on x. The spec says prefer closest on perpendicular
        // axis. Both are 150 away from center y=300. Either is acceptable, but
        // we just check that one of them is returned.
        let panes = three_pane_asymmetric();
        let result = find_pane_in_direction(&panes, PaneId::new(1), Direction::Right);
        assert!(
            result == Some(PaneId::new(2)) || result == Some(PaneId::new(3)),
            "expected pane 2 or 3, got {result:?}"
        );
    }

    #[test]
    fn asymmetric_move_left_from_top_right() {
        let panes = three_pane_asymmetric();
        let result = find_pane_in_direction(&panes, PaneId::new(2), Direction::Left);
        assert_eq!(result, Some(PaneId::new(1)));
    }

    #[test]
    fn asymmetric_move_down_from_top_right() {
        let panes = three_pane_asymmetric();
        let result = find_pane_in_direction(&panes, PaneId::new(2), Direction::Down);
        assert_eq!(result, Some(PaneId::new(3)));
    }

    // ============================================================
    // Edge case: single pane
    // ============================================================

    #[test]
    fn single_pane_all_directions_return_none() {
        let panes = vec![make_pane(1, 0.0, 0.0, 800.0, 600.0)];
        for dir in [Direction::Left, Direction::Right, Direction::Up, Direction::Down] {
            assert_eq!(
                find_pane_in_direction(&panes, PaneId::new(1), dir),
                None,
                "single pane, direction {dir:?} should return None"
            );
        }
    }

    // ============================================================
    // Edge case: at edges
    // ============================================================

    #[test]
    fn at_left_edge_move_left_returns_none() {
        let panes = two_pane_horizontal();
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(1), Direction::Left), None);
    }

    #[test]
    fn at_right_edge_move_right_returns_none() {
        let panes = two_pane_horizontal();
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(2), Direction::Right), None);
    }

    #[test]
    fn at_top_edge_move_up_returns_none() {
        let panes = two_pane_vertical();
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(1), Direction::Up), None);
    }

    #[test]
    fn at_bottom_edge_move_down_returns_none() {
        let panes = two_pane_vertical();
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(2), Direction::Down), None);
    }

    // ============================================================
    // Edge case: focused pane not in list
    // ============================================================

    #[test]
    fn focused_pane_not_found_returns_none() {
        let panes = two_pane_horizontal();
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(999), Direction::Right), None);
    }

    // ============================================================
    // Edge case: empty panes slice
    // ============================================================

    #[test]
    fn empty_panes_returns_none() {
        let panes: Vec<PaneLayout> = vec![];
        assert_eq!(find_pane_in_direction(&panes, PaneId::new(1), Direction::Right), None);
    }

    // ============================================================
    // Edge case: degenerate zero-size rects
    // ============================================================

    #[test]
    fn zero_size_rects_no_panic() {
        let panes = vec![make_pane(1, 0.0, 0.0, 0.0, 0.0), make_pane(2, 0.0, 0.0, 0.0, 0.0)];
        // Should not panic; result is deterministic (both centers at same point)
        let _ = find_pane_in_direction(&panes, PaneId::new(1), Direction::Right);
    }

    // ============================================================
    // Complex: deep nesting (6+ panes)
    // ============================================================

    #[test]
    fn six_pane_navigation_finds_nearest() {
        // Layout:
        // +-----+-----+-----+
        // |  1  |  2  |  3  |
        // +-----+-----+-----+
        // |  4  |  5  |  6  |
        // +-----+-----+-----+
        let panes = vec![
            make_pane(1, 0.0, 0.0, 266.0, 300.0),
            make_pane(2, 266.0, 0.0, 268.0, 300.0),
            make_pane(3, 534.0, 0.0, 266.0, 300.0),
            make_pane(4, 0.0, 300.0, 266.0, 300.0),
            make_pane(5, 266.0, 300.0, 268.0, 300.0),
            make_pane(6, 534.0, 300.0, 266.0, 300.0),
        ];

        // From center pane (5), navigate in all directions
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(5), Direction::Left),
            Some(PaneId::new(4))
        );
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(5), Direction::Right),
            Some(PaneId::new(6))
        );
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(5), Direction::Up),
            Some(PaneId::new(2))
        );
        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(5), Direction::Down),
            None, // pane 5 is in the bottom row
        );
    }

    // Correction: pane 5 IS in the bottom row, so Down returns None. Let's
    // also check from pane 2 (top center) going down to pane 5.
    #[test]
    fn six_pane_top_center_down_goes_to_bottom_center() {
        let panes = vec![
            make_pane(1, 0.0, 0.0, 266.0, 300.0),
            make_pane(2, 266.0, 0.0, 268.0, 300.0),
            make_pane(3, 534.0, 0.0, 266.0, 300.0),
            make_pane(4, 0.0, 300.0, 266.0, 300.0),
            make_pane(5, 266.0, 300.0, 268.0, 300.0),
            make_pane(6, 534.0, 300.0, 266.0, 300.0),
        ];

        assert_eq!(
            find_pane_in_direction(&panes, PaneId::new(2), Direction::Down),
            Some(PaneId::new(5))
        );
    }
}

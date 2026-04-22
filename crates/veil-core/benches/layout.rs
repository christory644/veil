//! Benchmarks for layout computation.
//!
//! Measures time to compute pixel rectangles for pane trees of various sizes.

use criterion::{criterion_group, criterion_main, Criterion};

use veil_core::layout::{compute_layout, Rect};
use veil_core::workspace::{PaneId, PaneNode, SplitDirection, SurfaceId};

fn leaf(pane_id: u64, surface_id: u64) -> PaneNode {
    PaneNode::Leaf { pane_id: PaneId::new(pane_id), surface_id: SurfaceId::new(surface_id) }
}

fn full_rect() -> Rect {
    Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 }
}

/// Build a 2-pane tree (single horizontal split).
fn tree_2_panes() -> PaneNode {
    PaneNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.5,
        first: Box::new(leaf(1, 1)),
        second: Box::new(leaf(2, 2)),
    }
}

/// Build a 6-pane tree (3 horizontal, each split vertically).
fn tree_6_panes() -> PaneNode {
    let col = |base: u64| PaneNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        first: Box::new(leaf(base, base)),
        second: Box::new(leaf(base + 1, base + 1)),
    };
    PaneNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.33,
        first: Box::new(col(1)),
        second: Box::new(PaneNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(col(3)),
            second: Box::new(col(5)),
        }),
    }
}

/// Build a 20-pane tree (chain of alternating horizontal/vertical splits).
fn tree_20_panes() -> PaneNode {
    let mut tree = leaf(1, 1);
    for i in 2..=20 {
        let direction =
            if i % 2 == 0 { SplitDirection::Horizontal } else { SplitDirection::Vertical };
        tree = PaneNode::Split {
            direction,
            ratio: 0.5,
            first: Box::new(tree),
            second: Box::new(leaf(i, i)),
        };
    }
    tree
}

/// Benchmark: `compute_layout` with 2 panes.
fn bench_layout_2_panes(c: &mut Criterion) {
    let tree = tree_2_panes();
    let rect = full_rect();
    c.bench_function("compute_layout (2 panes)", |b| {
        b.iter(|| {
            let _ = compute_layout(&tree, rect, None);
        });
    });
}

/// Benchmark: `compute_layout` with 6 panes.
fn bench_layout_6_panes(c: &mut Criterion) {
    let tree = tree_6_panes();
    let rect = full_rect();
    c.bench_function("compute_layout (6 panes)", |b| {
        b.iter(|| {
            let _ = compute_layout(&tree, rect, None);
        });
    });
}

/// Benchmark: `compute_layout` with 20 panes.
fn bench_layout_20_panes(c: &mut Criterion) {
    let tree = tree_20_panes();
    let rect = full_rect();
    c.bench_function("compute_layout (20 panes)", |b| {
        b.iter(|| {
            let _ = compute_layout(&tree, rect, None);
        });
    });
}

/// Benchmark: `compute_layout` with zoom vs without zoom on a 6-pane tree.
fn bench_layout_zoomed_vs_normal(c: &mut Criterion) {
    let tree = tree_6_panes();
    let rect = full_rect();

    c.bench_function("compute_layout (6 panes, no zoom)", |b| {
        b.iter(|| {
            let _ = compute_layout(&tree, rect, None);
        });
    });

    c.bench_function("compute_layout (6 panes, zoomed)", |b| {
        b.iter(|| {
            let _ = compute_layout(&tree, rect, Some(PaneId::new(3)));
        });
    });
}

criterion_group!(
    benches,
    bench_layout_2_panes,
    bench_layout_6_panes,
    bench_layout_20_panes,
    bench_layout_zoomed_vs_normal,
);
criterion_main!(benches);

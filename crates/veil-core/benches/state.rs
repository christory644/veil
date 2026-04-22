//! Benchmarks for `AppState` operations.
//!
//! Measures throughput of workspace creation, destruction, and pane splits.

use criterion::{criterion_group, criterion_main, Criterion};

use std::path::PathBuf;
use veil_core::state::AppState;
use veil_core::workspace::SplitDirection;

/// Benchmark: create + close workspace cycle.
fn bench_create_close_workspace(c: &mut Criterion) {
    c.bench_function("create_workspace + close_workspace cycle", |b| {
        b.iter(|| {
            let mut state = AppState::new();
            let id = state.create_workspace("bench".to_string(), PathBuf::from("/tmp"));
            let _ = state.close_workspace(id);
        });
    });
}

/// Benchmark: build a 20-pane workspace via `split_pane` chain.
fn bench_split_pane_chain_20(c: &mut Criterion) {
    c.bench_function("split_pane chain (20 panes)", |b| {
        b.iter(|| {
            let mut state = AppState::new();
            let ws_id = state.create_workspace("bench".to_string(), PathBuf::from("/tmp"));
            let first_pane = state.workspace(ws_id).unwrap().pane_ids()[0];
            let mut last_pane = first_pane;
            for _ in 1..20 {
                let (new_pane, _) =
                    state.split_pane(ws_id, last_pane, SplitDirection::Horizontal).expect("split");
                last_pane = new_pane;
            }
        });
    });
}

/// Benchmark: `create_workspace` repeatedly (measures ID generation overhead).
fn bench_create_many_workspaces(c: &mut Criterion) {
    c.bench_function("create_workspace x50", |b| {
        b.iter(|| {
            let mut state = AppState::new();
            for i in 0..50 {
                state.create_workspace(format!("ws-{i}"), PathBuf::from("/tmp"));
            }
        });
    });
}

criterion_group!(
    benches,
    bench_create_close_workspace,
    bench_split_pane_chain_20,
    bench_create_many_workspaces,
);
criterion_main!(benches);

//! Benchmarks for `SQLite` session store operations.
//!
//! Measures throughput of CRUD and search operations at various scales.

use criterion::{criterion_group, criterion_main, Criterion};

use chrono::Utc;
use std::path::PathBuf;
use veil_aggregator::store::SessionStore;
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionStatus};

fn make_entry(i: usize) -> SessionEntry {
    SessionEntry {
        id: SessionId::new(format!("bench-session-{i}")),
        agent: AgentKind::ClaudeCode,
        title: format!("Benchmark session {i}"),
        working_dir: PathBuf::from(format!("/tmp/bench/{i}")),
        branch: Some(format!("bench-branch-{i}")),
        pr_number: None,
        pr_url: None,
        plan_content: None,
        status: SessionStatus::Completed,
        started_at: Utc::now(),
        ended_at: Some(Utc::now()),
        indexed_at: Utc::now(),
    }
}

/// Benchmark: insert a single session via upsert.
fn bench_upsert_single(c: &mut Criterion) {
    c.bench_function("upsert_session (single)", |b| {
        let store = SessionStore::open_in_memory().expect("open store");
        let entry = make_entry(0);
        b.iter(|| {
            store.upsert_session(&entry).expect("upsert");
        });
    });
}

/// Benchmark: batch insert 100 sessions.
fn bench_upsert_batch_100(c: &mut Criterion) {
    c.bench_function("upsert_sessions (batch 100)", |b| {
        let store = SessionStore::open_in_memory().expect("open store");
        let entries: Vec<SessionEntry> = (0..100).map(make_entry).collect();
        b.iter(|| {
            store.upsert_sessions(&entries).expect("upsert batch");
        });
    });
}

/// Benchmark: `list_sessions` at various scales (10, 100, 1000 rows).
fn bench_list_sessions(c: &mut Criterion) {
    for count in [10, 100, 1000] {
        let store = SessionStore::open_in_memory().expect("open store");
        let entries: Vec<SessionEntry> = (0..count).map(make_entry).collect();
        store.upsert_sessions(&entries).expect("seed");

        c.bench_function(&format!("list_sessions ({count} rows)"), |b| {
            b.iter(|| {
                let _ = store.list_sessions().expect("list");
            });
        });
    }
}

/// Benchmark: FTS5 search against 100 indexed sessions.
fn bench_search_sessions(c: &mut Criterion) {
    let store = SessionStore::open_in_memory().expect("open store");
    let entries: Vec<SessionEntry> = (0..100).map(make_entry).collect();
    store.upsert_sessions(&entries).expect("seed");
    // Populate FTS index
    for entry in &entries {
        store
            .update_fts(&entry.id, &entry.title, Some("first user message text"))
            .expect("update fts");
    }

    c.bench_function("search_sessions (FTS, 100 rows)", |b| {
        b.iter(|| {
            let _ = store.search_sessions("session").expect("search");
        });
    });
}

criterion_group!(
    benches,
    bench_upsert_single,
    bench_upsert_batch_100,
    bench_list_sessions,
    bench_search_sessions,
);
criterion_main!(benches);

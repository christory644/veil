//! Benchmarks for JSONL session file parsing.
//!
//! Measures parsing throughput for fixture files of varying complexity.

use criterion::{criterion_group, criterion_main, Criterion};

use std::path::PathBuf;
use veil_aggregator::claude_code::parser::parse_session_file;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/claude_code/testdata").join(name)
}

/// Benchmark: parse `simple_session.jsonl` (2 records).
fn bench_parse_simple_session(c: &mut Criterion) {
    let path = fixture_path("simple_session.jsonl");
    c.bench_function("parse_session_file (simple, 2 records)", |b| {
        b.iter(|| {
            let _ = parse_session_file(&path).expect("parse");
        });
    });
}

/// Benchmark: parse `multi_turn_session.jsonl` (~10 records).
fn bench_parse_multi_turn_session(c: &mut Criterion) {
    let path = fixture_path("multi_turn_session.jsonl");
    c.bench_function("parse_session_file (multi-turn)", |b| {
        b.iter(|| {
            let _ = parse_session_file(&path).expect("parse");
        });
    });
}

/// Benchmark: parse `large_session.jsonl` (~50 records).
fn bench_parse_large_session(c: &mut Criterion) {
    let path = fixture_path("large_session.jsonl");
    c.bench_function("parse_session_file (large, ~50 records)", |b| {
        b.iter(|| {
            let _ = parse_session_file(&path).expect("parse");
        });
    });
}

criterion_group!(
    benches,
    bench_parse_simple_session,
    bench_parse_multi_turn_session,
    bench_parse_large_session,
);
criterion_main!(benches);

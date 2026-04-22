//! Benchmarks for JSONL session file parsing.
//!
//! Measures parsing throughput for fixture files of varying complexity.

use criterion::{criterion_group, criterion_main, Criterion};

use std::path::PathBuf;
use veil_aggregator::claude_code::parser::parse_session_file;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/claude_code/testdata").join(name)
}

fn bench_parse_fixtures(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("simple_session.jsonl", "parse_session_file (simple, 2 records)"),
        ("multi_turn_session.jsonl", "parse_session_file (multi-turn)"),
        ("large_session.jsonl", "parse_session_file (large, ~50 records)"),
    ];
    for (filename, label) in cases {
        let path = fixture_path(filename);
        c.bench_function(label, |b| {
            b.iter(|| {
                let _ = parse_session_file(&path).expect("parse");
            });
        });
    }
}

criterion_group!(benches, bench_parse_fixtures);
criterion_main!(benches);

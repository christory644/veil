# VEI-5: Project Scaffolding — Rust Workspace, Build System, CI

## Context

This task creates the foundational Rust project structure for Veil. Nothing can be built, tested, or integrated until the Cargo workspace, crate layout, build system scaffold, CI pipeline, and quality tooling are in place. Every subsequent VEI task depends on this scaffolding existing and compiling cleanly.

The crate layout follows `docs/system_design/system_design.md` and `AGENTS.md`. Crates live under `crates/` per the convention established in `.coderabbit.yaml`.

## Implementation Units

### Unit 1: Workspace Cargo.toml and Crate Skeletons

Create root `Cargo.toml` workspace manifest and skeleton `Cargo.toml` + `src/lib.rs` (or `src/main.rs`) for each crate.

**Files:**
- `Cargo.toml` (workspace root)
- `crates/veil/Cargo.toml` + `crates/veil/src/main.rs`
- `crates/veil-core/Cargo.toml` + `crates/veil-core/src/lib.rs`
- `crates/veil-ghostty/Cargo.toml` + `crates/veil-ghostty/src/lib.rs`
- `crates/veil-ui/Cargo.toml` + `crates/veil-ui/src/lib.rs`
- `crates/veil-pty/Cargo.toml` + `crates/veil-pty/src/lib.rs`
- `crates/veil-aggregator/Cargo.toml` + `crates/veil-aggregator/src/lib.rs`
- `crates/veil-socket/Cargo.toml` + `crates/veil-socket/src/lib.rs`

**Key decisions:**
- Workspace `[workspace.dependencies]` for version pinning: `thiserror`, `anyhow`, `tracing`, `serde`, `tokio`
- Workspace-level clippy lints via `[workspace.lints.clippy]`
- Each library crate: `#![deny(unsafe_code)]` `#![warn(missing_docs)]`
- Binary crate: `#![deny(unsafe_code)]`, prints version on run
- `veil-ghostty` exception: `#![deny(unsafe_op_in_unsafe_fn)]` instead of `deny(unsafe_code)`
- Intra-crate deps: all library crates depend on `veil-core`; binary crate depends on all

**Tests:**
1. `cargo build` succeeds with zero errors
2. `cargo build -p <crate>` succeeds for each crate
3. `cargo test` succeeds (0 tests is fine)
4. `cargo run` prints output containing "veil"
5. Library crates enforce `deny(unsafe_code)` except `veil-ghostty`

### Unit 2: Linting Configuration

Configure clippy and rustfmt at workspace level.

**Files:**
- `rustfmt.toml`
- `clippy.toml`
- Workspace `Cargo.toml` `[workspace.lints.clippy]` section
- Each crate: `[lints] workspace = true`

**Tests:**
1. `cargo fmt --check` passes
2. `cargo clippy --all-targets --all-features -- -D warnings` passes

### Unit 3: build.rs Scaffold for libghosty

Create `crates/veil-ghostty/build.rs` that handles missing libghosty gracefully (sets `no_libghosty` cfg flag, prints warning, does not fail build).

**Files:**
- `crates/veil-ghostty/build.rs`

**Tests:**
1. `cargo build -p veil-ghostty` succeeds with warning about missing libghosty
2. Full workspace build succeeds

### Unit 4: License Files

**Files:**
- `LICENSE-MIT`
- `LICENSE-APACHE`

**Tests:**
1. Both files exist
2. Content matches standard MIT and Apache-2.0 license text
3. `Cargo.toml` license field matches `"MIT OR Apache-2.0"`

### Unit 5: GitHub Actions CI

**Files:**
- `.github/workflows/ci.yml`

**Structure:** Matrix of `macos-latest`, `ubuntu-latest`, `windows-latest` with `cargo fmt --check`, `cargo clippy`, `cargo build`, `cargo test`. Triggers on push to `main`/`ralph-loop` and PRs to `main`. Uses `rust-cache` for speed, `fail-fast: false` for independent reporting.

**Tests:**
1. YAML is valid
2. Matrix covers all 3 OS targets
3. Includes all quality gate steps

### Unit 6: CONTRIBUTING.md

**Files:**
- `CONTRIBUTING.md`

**Sections:** Getting Started, Development Workflow (TDD cadence, quality gate), Writing an Agent Adapter, Code Review, License.

**Tests:**
1. File exists with adapter contribution guide content
2. References `AgentAdapter` trait and quality gate commands

### Unit 7: .gitignore Fix

Remove `Cargo.lock` from `.gitignore` — binary project must commit the lockfile per Rust convention.

**Tests:**
1. `Cargo.lock` is not gitignored
2. `target/` is still gitignored

## Acceptance Criteria

1. `cargo build` succeeds with zero errors/warnings
2. `cargo build -p <crate>` succeeds for all 7 crates
3. `cargo test` succeeds
4. `cargo clippy --all-targets --all-features -- -D warnings` passes
5. `cargo fmt --check` passes
6. `cargo run` executes cleanly
7. `LICENSE-MIT` and `LICENSE-APACHE` exist with correct content
8. `.github/workflows/ci.yml` defines a 3-OS matrix
9. `CONTRIBUTING.md` exists with adapter guide
10. `crates/veil-ghostty/build.rs` handles missing libghosty gracefully
11. All library crates enforce `deny(unsafe_code)` except `veil-ghostty`
12. `Cargo.lock` is committed
13. Full quality gate passes

## Dependencies

**Tooling:** Rust stable (1.92.0), Cargo — both already installed.

**External crates (workspace deps):** `thiserror` 2.x, `anyhow` 1.x, `tracing` 0.1.x, `serde` 1.x, `tokio` 1.x. Declared in workspace, only added to individual crate deps where actually used.

No external tools need to be installed. Zig/bindgen not needed until VEI-6.

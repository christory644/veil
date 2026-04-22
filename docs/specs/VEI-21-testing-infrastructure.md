# VEI-21: Testing Infrastructure

## Context

Veil's test suite serves as guardrails for agentic development -- AI agents build features with confidence because the test infrastructure catches regressions automatically. The codebase already has substantial unit tests (veil-core, veil-aggregator), property tests (layout module via proptest), test fixtures (JSONL testdata), and a basic CI pipeline. This task upgrades the testing infrastructure to be comprehensive:

- **CI pipeline**: Expand from a single `check` job to a full matrix with caching, coverage, and integration test gating
- **Test tooling**: Add mockall, criterion, and coverage reporting to the workspace
- **Test fixtures**: Create reusable fixture files (config TOML, workspace state) alongside existing JSONL fixtures
- **proptest expansion**: Add property-based tests for JSONL parsing, title generation, and state machine invariants
- **criterion benchmarks**: Set up benchmark scaffolding with initial benchmarks against existing code
- **E2E test harness**: Create scaffolding for socket-API-driven end-to-end tests

### What already exists

**CI** (`.github/workflows/ci.yml`): Single `check` job with 3-OS matrix, runs fmt/clippy/build/test. No caching beyond `Swatinem/rust-cache`, no coverage, no integration test gating.

**Unit tests**: Extensive coverage in veil-core (`workspace.rs`, `state.rs`, `layout.rs`, `navigation.rs`, `focus.rs`, `keyboard.rs`, `session.rs`, `lifecycle.rs`, `message.rs`) and veil-aggregator (`adapter.rs`, `store.rs`, `registry.rs`, `title.rs`, `jsonl.rs`, `parser.rs`). Also in veil binary crate (`vertex.rs`, `frame.rs`, `quad_builder.rs`, `renderer.rs`, `font/*.rs`, `sidebar_wiring.rs`) and veil-ui (`sidebar.rs`, `workspace_list.rs`, `conversation_list.rs`, `time_fmt.rs`).

**Property tests**: `proptest` is a workspace dependency, used in `veil-core/src/layout.rs` with custom strategies for `PaneNode` trees. Tests verify layout count, non-negative rects, total area conservation, and no-overlap invariants.

**Test fixtures**: Five JSONL fixture files in `crates/veil-aggregator/src/claude_code/testdata/` (simple_session, multi_turn_session, compact_summary_session, empty_session, malformed_lines).

**Mock adapter**: `crates/veil-aggregator/src/testutil.rs` has a hand-rolled `MockAdapter` for the `AgentAdapter` trait. The `mockall` crate is NOT currently in the workspace.

**Benchmarks**: None. `criterion` is NOT in the workspace.

**Coverage**: None configured.

**E2E tests**: None. `veil-socket` is an empty lib crate.

---

## Implementation Units

### Unit 1: Expand CI Pipeline

Upgrade `.github/workflows/ci.yml` from a single job to a multi-job pipeline with proper caching, coverage, and merge protection.

**Changes:**

1. **Restructure into multiple jobs**:
   - `lint`: fmt + clippy (runs on ubuntu-latest only -- formatting is platform-independent)
   - `test`: unit + property tests (3-OS matrix: macos-latest, ubuntu-latest, windows-latest)
   - `coverage`: cargo-llvm-cov on ubuntu-latest, upload to GitHub Actions artifacts
   - `integration`: feature-gated integration tests, only on ubuntu-latest, conditional on Zig availability (for future libghosty builds)

2. **Add Cargo caching** beyond `Swatinem/rust-cache`:
   - Cache `~/.cargo/registry/cache` and `~/.cargo/registry/src` across runs
   - Cache the `target/` directory keyed on `Cargo.lock` hash + OS
   - `Swatinem/rust-cache` already handles most of this; verify its configuration is optimal (set `shared-key`, `cache-on-failure: true`)

3. **Add coverage reporting**:
   - Install `cargo-llvm-cov` in the coverage job
   - Generate lcov output, upload as GitHub Actions artifact
   - Do NOT upload to external services (no Codecov/Coveralls for now)

4. **Add branch protection note**: Document in the workflow file that branch protection rules should be configured in GitHub settings to require `lint` and `test` jobs to pass before merge.

5. **Integration test gate**: Add a conditional step that checks for Zig availability; skip integration tests if Zig is not installed. Use a feature flag `integration` for tests that require real libghosty.

**Files:**
- `.github/workflows/ci.yml` (rewrite)

**Test strategy:**
- The scaffold test `ci_workflow_exists` and related tests in `veil-scaffold-tests` already verify CI structure. Add assertions for the new jobs (coverage, lint separation).
- Manual verification: push to `ralph-loop` branch, confirm all jobs pass.

---

### Unit 2: Add Workspace Dependencies (mockall, criterion, cargo-llvm-cov)

Add the missing test tooling dependencies to the workspace.

**Changes:**

1. **Add `mockall = "0.13"` to `[workspace.dependencies]`** in root `Cargo.toml`.
2. **Add `criterion = { version = "0.5", features = ["html_reports"] }` to `[workspace.dependencies]`**.
3. **Add `mockall` as a `[dev-dependencies]` in crates that will use it**: `veil-core`, `veil-aggregator`, `veil-pty`, `veil-socket`.
4. **Add `criterion` as a `[dev-dependencies]` in crates that will have benchmarks**: `veil-aggregator`, `veil-core`.
5. **Add `proptest` as a `[dev-dependencies]`** to `veil-aggregator` (it already depends on `serde_json` and `chrono` but does not have `proptest`).

**Files:**
- `Cargo.toml` (workspace root)
- `crates/veil-core/Cargo.toml`
- `crates/veil-aggregator/Cargo.toml`
- `crates/veil-pty/Cargo.toml`
- `crates/veil-socket/Cargo.toml`

**Test strategy:**
- `cargo build --all-targets` must succeed after adding dependencies.
- Verify with `cargo test` that no existing tests break.

---

### Unit 3: Test Fixture Files

Create reusable test fixture files for config parsing and workspace state. Extend existing JSONL fixtures with edge-case files.

**Changes:**

1. **Config TOML fixtures** in `crates/veil-core/src/testdata/`:
   - `valid_config.toml`: A complete valid Veil config file with all sections.
   - `minimal_config.toml`: A config with only required fields.
   - `invalid_config.toml`: Malformed TOML (syntax errors).
   - `unknown_fields_config.toml`: Valid TOML with extra unknown fields (forward compat).

   Note: The config system does not exist yet (no TOML parsing code in veil-core). These fixtures are scaffolding for future work but serve as canonical reference for the expected config format documented in `system_design.md`.

2. **Additional JSONL fixtures** in `crates/veil-aggregator/src/claude_code/testdata/`:
   - `unicode_content.jsonl`: Session with emoji, CJK characters, and multi-byte UTF-8 in messages.
   - `large_session.jsonl`: Session with ~50 records (tests performance at moderate scale without being too large for git).
   - `sidechain_session.jsonl`: Session containing `isSidechain: true` records to verify they are handled correctly.

3. **Workspace state fixture** in `crates/veil-core/src/testdata/`:
   - `workspace_state.json`: A persisted workspace state file matching the `PersistedState` structure from the system design doc. Scaffolding for future persistence tests.

**Files:**
- `crates/veil-core/src/testdata/valid_config.toml`
- `crates/veil-core/src/testdata/minimal_config.toml`
- `crates/veil-core/src/testdata/invalid_config.toml`
- `crates/veil-core/src/testdata/unknown_fields_config.toml`
- `crates/veil-core/src/testdata/workspace_state.json`
- `crates/veil-aggregator/src/claude_code/testdata/unicode_content.jsonl`
- `crates/veil-aggregator/src/claude_code/testdata/large_session.jsonl`
- `crates/veil-aggregator/src/claude_code/testdata/sidechain_session.jsonl`

**Test strategy:**
- **Config fixtures**: Verify files are valid/invalid TOML as intended using `toml::from_str` in a test (add `toml` as a dev-dependency to veil-core if not already present).
- **JSONL fixtures**: Write tests in `parser.rs` that parse the new fixtures and verify expected counts, Unicode content preservation, and sidechain handling.
- **Workspace state**: Verify JSON is valid and deserializable (add a test that reads and parses it using `serde_json`).

---

### Unit 4: Proptest Expansion

Add property-based tests to veil-aggregator for JSONL parsing resilience and to veil-core for state machine invariants beyond layout.

**Changes:**

1. **JSONL parsing property tests** in `crates/veil-aggregator/src/claude_code/jsonl.rs`:
   - Arbitrary byte sequences fed to `serde_json::from_str::<JournalRecord>` must never panic (they should return `Err`).
   - Arbitrary valid JSON objects with random `type` fields must not panic.
   - Valid user/assistant records with arbitrary string content in message fields must deserialize without panic.

2. **Title generation property tests** in `crates/veil-aggregator/src/title.rs`:
   - `generate_title` with arbitrary `Option<&str>` inputs must never panic and must always return a non-empty string.
   - `is_gibberish_title` with arbitrary strings must never panic.
   - `extract_topic_from_message` with arbitrary strings (including multi-byte Unicode, empty, very long) must never panic and must return a string with length <= 100 (80 + "..." suffix).

3. **AppState invariant property tests** in `crates/veil-core/src/state.rs`:
   - After any sequence of `create_workspace` / `close_workspace` / `set_active_workspace` operations, `active_workspace_id` is always either `None` or refers to an existing workspace.
   - After any sequence of `split_pane` / `close_pane` operations on a workspace, the pane tree has at least 1 leaf.
   - `next_id()` is strictly monotonically increasing across any number of calls.

4. **Workspace operation property tests** in `crates/veil-core/src/workspace.rs`:
   - After any sequence of splits and closes, `pane_ids()` length equals `pane_count()`.
   - Every `surface_id` returned by `surface_ids()` corresponds to a findable leaf via `find_pane`.

**Files:**
- `crates/veil-aggregator/src/claude_code/jsonl.rs` (add `#[cfg(test)] mod proptests` section)
- `crates/veil-aggregator/src/title.rs` (add `#[cfg(test)] mod proptests` section)
- `crates/veil-core/src/state.rs` (add `#[cfg(test)] mod proptests` section)
- `crates/veil-core/src/workspace.rs` (add `#[cfg(test)] mod proptests` section)

**Test strategy:**
- All property tests run as part of `cargo test`.
- Use `proptest!` macro with reasonable case counts (default 256, explicitly set via `ProptestConfig` for expensive tests).
- Use `prop_assert!` and `prop_assert_eq!` for assertions inside proptest blocks.
- Verify no panics, no infinite loops. Use timeouts where appropriate.

---

### Unit 5: Criterion Benchmark Scaffolding

Set up criterion benchmark infrastructure with initial benchmarks against existing code.

**Changes:**

1. **Aggregator benchmarks** in `crates/veil-aggregator/benches/`:
   - `session_store.rs`: Benchmark SQLite operations:
     - `upsert_session` (single insert)
     - `upsert_sessions` (batch of 100)
     - `list_sessions` (with 10, 100, 1000 pre-populated sessions)
     - `search_sessions` (FTS query against 100 indexed sessions)
   - `jsonl_parsing.rs`: Benchmark JSONL parsing:
     - `parse_session_file` against `simple_session.jsonl`
     - `parse_session_file` against `multi_turn_session.jsonl`
     - `parse_session_file` against `large_session.jsonl` (from Unit 3)

2. **Core benchmarks** in `crates/veil-core/benches/`:
   - `layout.rs`: Benchmark layout computation:
     - `compute_layout` with 2-pane, 6-pane, 20-pane trees
     - `compute_layout` with zoomed pane vs non-zoomed
   - `state.rs`: Benchmark state operations:
     - `create_workspace` + `close_workspace` cycle
     - `split_pane` chain (build 20-pane workspace)

3. **Benchmark configuration**: Each benchmark file uses `criterion_group!` and `criterion_main!` macros. Add `[[bench]]` entries to the relevant `Cargo.toml` files with `harness = false`.

**Files:**
- `crates/veil-aggregator/benches/session_store.rs`
- `crates/veil-aggregator/benches/jsonl_parsing.rs`
- `crates/veil-core/benches/layout.rs`
- `crates/veil-core/benches/state.rs`
- `crates/veil-aggregator/Cargo.toml` (add `[[bench]]` entries)
- `crates/veil-core/Cargo.toml` (add `[[bench]]` entries)

**Test strategy:**
- `cargo bench --no-run` must compile without errors.
- `cargo bench` runs benchmarks and produces output (no performance thresholds enforced yet -- this is infrastructure, not regression gating).
- Benchmarks should complete in reasonable time (< 30 seconds total for `cargo bench`).
- Each benchmark function should be documented with what it measures and why.

---

### Unit 6: Coverage Reporting Setup

Configure cargo-llvm-cov for local coverage reporting and CI integration.

**Changes:**

1. **Add a `coverage` script** to the project: a shell script or Makefile target that developers can run locally.
   - `.cargo/config.toml`: Add an alias `[alias] cov = "llvm-cov --lcov --output-path lcov.info"` so `cargo cov` generates coverage.
   - Document in AGENTS.md under "Build & Test Commands" how to generate coverage.

2. **CI integration** (handled in Unit 1): The CI `coverage` job installs `cargo-llvm-cov`, runs tests with coverage, and uploads the lcov report as a GitHub Actions artifact.

3. **Exclude files from coverage**: Configure `cargo-llvm-cov` to exclude benchmark files, scaffold tests, and test utilities from coverage metrics (via `--ignore-filename-regex`).

**Files:**
- `.cargo/config.toml` (create or update with alias)
- AGENTS.md (add coverage command documentation)

**Test strategy:**
- `cargo llvm-cov --no-run` (after installing the tool) must not error.
- CI coverage job produces a non-empty `lcov.info` artifact.
- Coverage report includes veil-core and veil-aggregator source files.

---

### Unit 7: E2E Test Harness Scaffolding

Create the structural scaffolding for socket-API-driven end-to-end tests. The socket API itself is not implemented yet (veil-socket is empty), so this unit creates the test harness framework that will be populated as the socket API is built.

**Changes:**

1. **Create `crates/veil-e2e/` crate**: A new test-only crate (not published) that depends on `veil-socket`, `veil-core`, `tokio`, and `serde_json`.

2. **Harness module** (`crates/veil-e2e/src/lib.rs`):
   - `VeilTestInstance` struct: Manages lifecycle of a test Veil instance.
     - `start()`: Will eventually launch a Veil process and wait for the socket to become available.
     - `stop()`: Sends shutdown signal and waits for clean exit.
     - `socket_path()`: Returns the path to the JSON-RPC socket.
   - `JsonRpcClient` struct: Sends JSON-RPC 2.0 requests over a Unix socket and reads responses.
     - `call(method: &str, params: serde_json::Value) -> Result<serde_json::Value>`
     - `notify(method: &str, params: serde_json::Value) -> Result<()>`
   - Both structs are initially stubs that return `todo!()` or `unimplemented!()` errors -- they exist to define the API shape.

   **Important exception to the "no placeholders" convention**: This unit intentionally uses placeholder implementations because the socket API it drives does not exist yet. The stubs define the test harness API surface so that socket API work (VEI-20) can immediately write E2E tests against a ready-made harness. Each stub has a doc comment explaining what it will do and which VEI task will implement it.

3. **Example E2E test** (`crates/veil-e2e/tests/smoke.rs`):
   - A `#[test] #[ignore]` test named `smoke_test_placeholder` that documents the E2E test pattern:
     ```rust
     // 1. Start a VeilTestInstance
     // 2. Connect JsonRpcClient
     // 3. Call workspace.list, assert empty
     // 4. Call workspace.create, assert success
     // 5. Call workspace.list, assert 1 workspace
     // 6. Stop instance
     ```
   - Marked `#[ignore]` so `cargo test` doesn't fail on the stubs. CI can run with `--include-ignored` when the harness is ready.

**Files:**
- `crates/veil-e2e/Cargo.toml`
- `crates/veil-e2e/src/lib.rs`
- `crates/veil-e2e/tests/smoke.rs`
- `Cargo.toml` (add `crates/veil-e2e` to workspace members)

**Test strategy:**
- `cargo build -p veil-e2e` must compile.
- `cargo test -p veil-e2e` must pass (the `#[ignore]` test is skipped by default).
- The crate's public API (`VeilTestInstance`, `JsonRpcClient`) must compile and have doc comments.

---

### Unit 8: Update AGENTS.md and Scaffold Tests

Update project documentation and scaffold tests to reflect the new testing infrastructure.

**Changes:**

1. **AGENTS.md updates**:
   - Add `veil-e2e` to the project structure diagram.
   - Add benchmark and coverage commands to "Build & Test Commands":
     ```
     cargo bench                      # Run benchmarks
     cargo bench -p veil-aggregator   # Benchmark one crate
     cargo llvm-cov                   # Generate coverage report
     ```
   - Update the Tech Stack table to include `mockall` under Testing.
   - Add a note about the `#[ignore]` convention for E2E tests.

2. **Scaffold test updates** in `veil-scaffold-tests/src/lib.rs`:
   - Add `veil-e2e` to `EXPECTED_CRATES` (as a test-only crate).
   - Add a test verifying that `benches/` directories exist in `veil-core` and `veil-aggregator`.
   - Add a test verifying that `.cargo/config.toml` exists with the coverage alias.

3. **veil-scaffold-tests/Cargo.toml**: No changes needed (it has no dependencies since it only reads files).

**Files:**
- `AGENTS.md`
- `crates/veil-scaffold-tests/src/lib.rs`

**Test strategy:**
- All existing scaffold tests must continue to pass.
- New scaffold tests must pass after all other units are implemented.
- `cargo test -p veil-scaffold-tests` is green.

---

## Acceptance Criteria

1. **CI pipeline** (`.github/workflows/ci.yml`):
   - Separate `lint`, `test`, and `coverage` jobs
   - 3-OS matrix for `test` job (macOS, Linux, Windows)
   - `Swatinem/rust-cache` with optimized configuration
   - Coverage job generates lcov output and uploads as artifact
   - Integration test step gated on Zig availability
   - All jobs pass on push to `ralph-loop`

2. **Dependencies**:
   - `mockall`, `criterion`, `proptest` available as workspace dependencies
   - Each crate that needs them has them in `[dev-dependencies]`
   - `cargo build --all-targets` and `cargo test` pass

3. **Test fixtures**:
   - Config TOML fixtures exist and are validated by tests
   - New JSONL fixtures (unicode, large, sidechain) exist and are parsed by tests
   - Workspace state JSON fixture exists

4. **Property-based tests**:
   - JSONL parsing: arbitrary input never panics
   - Title generation: arbitrary input never panics, output is always non-empty
   - AppState invariants: active workspace is always valid or None after any operation sequence
   - Workspace invariants: pane tree consistency after arbitrary split/close sequences
   - All property tests pass under `cargo test`

5. **Criterion benchmarks**:
   - Benchmark files exist in `crates/veil-aggregator/benches/` and `crates/veil-core/benches/`
   - `cargo bench --no-run` compiles successfully
   - `cargo bench` produces output with timing data
   - `[[bench]]` entries in Cargo.toml files

6. **Coverage**:
   - `.cargo/config.toml` has `cov` alias
   - `cargo llvm-cov` (when installed) generates coverage report
   - AGENTS.md documents the command

7. **E2E harness**:
   - `veil-e2e` crate compiles
   - `VeilTestInstance` and `JsonRpcClient` types exist with doc comments
   - Smoke test exists as `#[ignore]` test
   - `cargo test -p veil-e2e` passes (ignored tests skipped)

8. **Documentation and scaffolding**:
   - AGENTS.md updated with new commands and crate
   - Scaffold tests verify new infrastructure

---

## Dependencies

| Dependency | Version | Purpose | Where |
|------------|---------|---------|-------|
| `mockall` | 0.13 | Trait mocking for unit tests | workspace dep, dev-dep in veil-core, veil-aggregator, veil-pty, veil-socket |
| `criterion` | 0.5 | Benchmark framework | workspace dep, dev-dep in veil-core, veil-aggregator |
| `proptest` | 1 | Property-based testing | already workspace dep; add as dev-dep in veil-aggregator |
| `cargo-llvm-cov` | latest | Coverage reporting | installed via `cargo install` or `cargo binstall`, not a Cargo.toml dependency |
| `toml` | 0.8 | Parsing config fixture validation tests | dev-dep in veil-core |
| `serde_json` | 1 | E2E harness JSON-RPC, fixture validation | already workspace dep |
| `tokio` | 1 | E2E harness async runtime | already workspace dep |
| `tempfile` | 3 | E2E harness temp socket paths | already workspace dep |

**External tooling (not Cargo dependencies):**
- `cargo-llvm-cov`: Installed in CI via `cargo install cargo-llvm-cov` or `taiki-e/install-action@cargo-llvm-cov`
- GitHub Actions: `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `actions/upload-artifact@v4`

---

## Implementation Order

Units can be implemented mostly in parallel, but some have soft dependencies:

1. **Unit 2** (workspace deps) -- must go first, other units need the dependencies
2. **Unit 3** (fixtures) -- needed by Unit 4 (proptest for new fixtures) and Unit 5 (benchmarks for large_session.jsonl)
3. **Units 4, 5, 6, 7** can proceed in parallel after 2 and 3
4. **Unit 1** (CI) can proceed in parallel but should incorporate Unit 6 (coverage)
5. **Unit 8** (docs/scaffold) goes last, verifying everything else

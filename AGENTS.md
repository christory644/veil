# Veil — Agent Operational Guide

## What Is This Project?

Veil is a cross-platform, GPU-accelerated terminal workspace manager built on libghosty (from Ghostty), purpose-built for developers working with AI coding agents. See `docs/` for full specs.

## Project Structure

Cargo workspace with these crates:

```
veil/                   # Binary crate — app entry point
veil-core/              # State management, workspace/session types, AppState
veil-ghostty/           # Safe libghosty FFI wrapper (C ABI → Rust)
veil-pty/               # Cross-platform PTY abstraction
veil-ui/                # egui navigation pane (sidebar, tabs)
veil-aggregator/        # Session aggregator + agent adapters + SQLite
veil-socket/            # JSON-RPC socket API server
```

## Build & Test Commands

```bash
# Build all crates
cargo build

# Build a specific crate
cargo build -p veil-core

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p veil-aggregator

# Run with clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --check

# Run the binary (once it exists)
cargo run
```

## Tech Stack

| Layer | Crate | Key Dependencies |
|-------|-------|-----------------|
| Terminal | veil-ghostty | libghosty (C FFI via rust-bindgen) |
| Rendering | (in veil binary) | wgpu, winit |
| Font | (in veil binary) | swash, rustybuzz |
| Sidebar UI | veil-ui | egui |
| PTY | veil-pty | libc (macOS/Linux), winapi (Windows) |
| Session cache | veil-aggregator | rusqlite, notify |
| Socket API | veil-socket | tokio, serde_json |
| Config | veil-core | toml, serde, notify |
| Observability | (all crates) | tracing |
| Testing | (all crates) | proptest, criterion, mockall |

## Conventions

- **Testing**: TDD — write tests FIRST. Use `#[test]` for unit tests, `proptest` for property-based, `criterion` for benchmarks.
- **Error handling**: Use `thiserror` for library crates, `anyhow` for the binary. Never silently swallow errors.
- **Logging**: Use `tracing` with appropriate levels (ERROR for component failures, WARN for degraded, INFO for lifecycle, DEBUG for state transitions).
- **FFI safety**: All libghosty FFI calls wrapped in `catch_unwind`. Raw pointers never escape the `veil-ghostty` crate.
- **No placeholders**: Every function must have a real implementation, not stubs/TODOs.
- **Commit style**: Conventional commits (feat:, fix:, refactor:, test:, docs:, chore:).

## Linear Integration

The task backlog lives in Linear (team: Veil-term, prefix: VEI-).
- Query tasks: use `mcp__linear-server__list_issues` with team "Veil-term"
- Check blocking: issues have `blockedBy` relationships
- Update status: move to "In Progress" when starting, "Done" when complete
- Create issues: if you discover missing work, create a new issue in Linear

## Git Workflow

- Work happens on feature branches (from the worktree)
- Commit after each successful test cycle
- Push to remote when a task is complete
- Use conventional commit messages

## Design Docs

Read these for context on any implementation work:
- `docs/prd/prd.md` — Product requirements, feature priorities
- `docs/ui_design/ui_design.md` — Layout, navigation, keyboard shortcuts
- `docs/system_design/system_design.md` — Architecture, all technical decisions

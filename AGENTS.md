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
cargo build                                                  # Build all
cargo build -p veil-core                                     # Build one crate
cargo test                                                   # Test all
cargo test -p veil-aggregator                                # Test one crate
cargo clippy --all-targets --all-features -- -D warnings     # Lint
cargo fmt --check                                            # Format check
cargo run                                                    # Run binary
```

## Quality Gate (MUST pass before every commit)

Every commit must pass all four checks. No exceptions. Run them in this order:

```bash
cargo fmt                                                    # Auto-format first
cargo clippy --all-targets --all-features -- -D warnings     # Then lint
cargo test                                                   # Then test
cargo build                                                  # Then full build
```

If any check fails, fix the issue before committing. Never create a separate commit for formatting or lint fixes — those should never be needed if the gate is followed.

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
| Testing | (all crates) | proptest, criterion |

## Conventions

- **Testing**: Strict TDD — write failing test first, make it pass, refactor. Use `#[test]` for unit tests, `proptest` for property-based, `criterion` for benchmarks. Aim for comprehensive coverage across all test types.
- **Error handling**: Use `thiserror` for library crates, `anyhow` for the binary. Never silently swallow errors.
- **Logging**: Use `tracing` with appropriate levels (ERROR for component failures, WARN for degraded, INFO for lifecycle, DEBUG for state transitions).
- **FFI safety**: All libghosty FFI calls wrapped in `catch_unwind`. Raw pointers never escape the `veil-ghostty` crate.
- **No placeholders**: Every function must have a real implementation, not stubs/TODOs.
- **Commit style**: Conventional commits (test:, feat:, refactor:, fix:). See "TDD Commit Cadence" below.
- **Single responsibility per file**: Each file should express one cohesive unit. The signal to split is mixed responsibilities, not line count. Most files will naturally land around ~300 lines, but a module that genuinely needs more to express a single unit cleanly is fine.
- **Documentation**: Keep docs close to the code. Use inline comments for implementation details. Only create markdown docs for large architectural concepts.

## TDD Commit Cadence

Every unit of work produces 2-3 commits that tell the TDD story. **Include the Linear issue number in the parenthetical** so commits are traceable to tasks:

1. `plan(VEI-14): design spec for <feature>` — The implementation spec committed as an auditable artifact at `docs/specs/`.
2. `test(VEI-14): RED for <describe the behavior>` — The failing tests (RED). Commit this so the log shows what you intended.
3. `feat(VEI-14): implement <unit name>` or `fix(VEI-14): <what was broken>` — The minimal implementation to pass (GREEN). Include any doc changes that belong with this code change.
4. `refactor(VEI-14): <what was improved>` — Clean up, split files with mixed concerns, reduce complexity (REFACTOR). This step is NOT optional — every task should include refactoring.

This cadence produces an auditable commit log. Never create standalone `docs:`, `style:`, or `chore:` commits.

## Code Review Gate

Before pushing, the main agent spawns review subagents to run CodeRabbit:

```bash
coderabbit review --plain --type committed --base-commit <commit-before-task-started>
```

Read the feedback and fix legitimate issues (logic errors, missing tests, unsafe misuse, resource leaks, API design problems). Ignore feedback about formatting or clippy lints — those are already gated. If you disagree with a suggestion, note why in your commit message. Default to fixing; the bar for ignoring is "doesn't apply," not "inconvenient." After fixing, run coderabbit again and iterate until clean. Then push.

## Linear Integration

The task backlog lives in Linear (team: Veil-term, prefix: VEI-).
- Query tasks: use `mcp__linear-server__list_issues` with team "Veil-term"
- Check blocking: issues have `blockedBy` relationships
- Update status: move to "In Progress" when starting, "Done" when complete
- **Leave a comment** on every status change explaining what was done, what was deferred, and any follow-up work identified
- Do NOT create new Linear issues. The backlog is curated by the human operator.

## Specs

Per-task implementation specs are committed to `docs/specs/` as auditable artifacts. Each spec contains context, implementation units, test strategy, acceptance criteria, and dependencies. These are produced by the planning subagent during Phase 2 of the ralph loop.

## Design Docs

Read these for context on any implementation work:
- `docs/prd/prd.md` — Product requirements, feature priorities
- `docs/ui_design/ui_design.md` — Layout, navigation, keyboard shortcuts
- `docs/system_design/system_design.md` — Architecture, all technical decisions

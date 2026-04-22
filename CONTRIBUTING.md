# Contributing to Veil

Thank you for your interest in contributing to Veil! This guide covers everything you need to get started.

## Getting Started

1. **Clone the repository:**

   ```bash
   git clone https://github.com/veil-term/veil.git
   cd veil
   ```

2. **Install Rust (stable):**

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. **Build the project:**

   ```bash
   cargo build
   ```

4. **Run the tests:**

   ```bash
   cargo test
   ```

If everything compiles and the tests pass, you're ready to contribute.

## Development Workflow

Veil follows a **TDD (test-driven development)** cadence: write a failing test first, make it pass with the simplest implementation, then refactor.

### Quality Gate

Before pushing any changes, run the full quality gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --all-targets
```

All four commands must pass cleanly. CI enforces this on every PR.

### Commit Conventions

- Write clear, concise commit messages that describe the *why*, not just the *what*.
- Prefix with a conventional type: `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`.
- Keep commits atomic -- one logical change per commit.

## Writing an Agent Adapter

Veil's session aggregator discovers and normalizes conversation sessions from various AI coding agents (Claude Code, Codex, OpenCode, etc.). Each agent is supported via an adapter that implements the `AgentAdapter` trait.

### Step-by-step guide

1. **Create a new module** in `crates/veil-aggregator/src/adapters/`. Name it after the agent (e.g., `claude_code.rs`).

2. **Implement the `AgentAdapter` trait.** The trait requires:
   - `name()` -- returns the agent's display name.
   - `discover_sessions()` -- scans the filesystem (or other sources) for session data.
   - `parse_session()` -- parses raw session data into Veil's normalized `Session` type.

3. **Handle errors gracefully.** Use `anyhow::Result` for fallible operations. If the agent's data directory doesn't exist, return an empty list rather than erroring.

4. **Write tests.** Every adapter must have unit tests covering:
   - Discovery of sessions from fixture data.
   - Correct parsing of session metadata (timestamps, message counts, etc.).
   - Graceful handling of missing or malformed data.

5. **Register the adapter** in the aggregator's adapter registry so it is included in session discovery.

## Code Review

All changes go through pull request review before merging.

- PRs should include a clear description of *what* changed and *why*.
- Automated review via coderabbit runs on every PR to catch common issues.
- At least one maintainer approval is required before merging.
- Keep PRs focused -- prefer small, reviewable changesets over large monolithic PRs.

## License

Veil is dual-licensed under **MIT OR Apache-2.0**. By contributing, you agree that your contributions will be licensed under the same terms.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

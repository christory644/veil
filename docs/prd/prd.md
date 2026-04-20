# Veil — Product Requirements Document

## Problem Statement

Developers increasingly work with multiple AI coding agents (Claude Code, Codex, OpenCode, Pi, etc.) across multiple projects simultaneously. The current terminal multiplexer landscape fails to serve this workflow:

- **cmux** is the closest product — a terminal workspace manager built on libghosty with AI agent awareness — but it is **closed-source**, **macOS-only**, and has **no conversation session history**. You cannot use it on Windows (native or WSL) or Linux.
- **tmux/zellij/wezterm** are general-purpose terminal multiplexers with no awareness of AI agent sessions or workflows.
- **Claude Desktop / Codex Desktop** have excellent conversation history navigation but are standalone apps, not terminal environments where the actual agent work happens.

There is no cross-platform terminal workspace manager that combines workspace management with AI agent session history navigation.

## Vision

**Veil** is a cross-platform, GPU-accelerated terminal workspace manager built on libghosty, purpose-built for developers working with AI coding agents. Its key differentiator is a **tabbed navigation pane** that provides both workspace management (like cmux) and conversation session history browsing (like Claude Desktop), unified in a single terminal application.

## Target Users

- Developers who use AI coding agents (Claude Code, Codex, OpenCode, Pi) as core parts of their workflow
- Users who work across multiple projects/repos simultaneously
- Users who want to navigate, review, and resume past agent conversations
- Users on macOS, Linux, **and Windows** who want a consistent experience

## Key Features

### P0 — Core (MVP)

1. **Terminal emulation via libghosty** — GPU-accelerated, full-featured VT terminal
2. **Workspace management** — Multiple workspaces, each with configurable split panes
3. **Tabbed navigation pane** with two tabs:
   - **Workspaces tab** — Workspace list with git branch, working directory, notifications, PR status
   - **Conversations tab** — Session history browser grouped by agent harness, with meaningful titles, branch/PR metadata, and timestamps
4. **Cross-platform** — macOS, Linux, Windows (native + WSL)
5. **Keyboard-driven navigation** — Fast switching between workspaces, panes, and conversation history
6. **Workspace persistence** — Configurable save/restore of workspaces across restarts (directories, splits, layout). First-run exit prompt lets user choose behavior, with option to save preference.
7. **Configuration hot-reload** — Config changes (font, theme, keybindings, sidebar) apply live without restart

### P1 — Agent Integration

8. **Session data aggregation** — Read session history from multiple agent harnesses:
   - Claude Code (`~/.claude/projects/<project>/`) — first adapter, format known
   - Codex (data store TBD — needs research)
   - OpenCode (data store TBD — needs research)
   - Pi (data store TBD — needs research)
9. **Socket API** — JSON-RPC over Unix domain socket (named pipes on Windows) for programmatic control by agents and extensions
10. **Notification system** — Visual notifications from agents via OSC escape sequences and socket API
11. **Session resume** — Jump to a workspace running an agent and resume a previous conversation
12. **Progressive conversation metadata** — Conversations enrich over time (branch created, PR opened, plan finalized) by observing agent activity via session data and PTY output

### P2 — Polish

13. **Configuration system** — TOML config for themes, keybindings, sidebar preferences
14. **Ghostty config compatibility** — Read existing Ghostty font/color/theme settings
15. **Shell integration** — Directory tracking, command detection, environment awareness
16. **Update notifications** — Detect new versions, prompt user to update via their package manager

### P3 — Future

17. **Plugin architecture** — Extract internal modules into WASM-based plugin system for third-party extensibility
18. **Pre-built binaries** — Downloadable releases for users not using package managers
19. **Auto-update** — Self-updating mechanism (works regardless of install method)
20. **Additional agent adapters** — Community-contributed adapters for new harnesses

## Non-Goals

- **Not an IDE** — No built-in editor, file tree, or language server integration
- **Not an AI agent** — Veil orchestrates and navigates agent sessions, it doesn't run its own AI
- **Not a tmux replacement for servers** — Veil is a desktop GUI application, not a headless multiplexer
- **Not a browser** — Unlike cmux, we're not shipping an embedded browser (at least not in MVP)
- **Not a plugin platform (at MVP)** — Internal modules first; plugin architecture evolves post-launch

## Agent Harness Integrations

Each agent harness stores conversation/session data differently. Veil needs adapters for each:

| Agent Harness | Session Data Location | Format | Status |
|---------------|----------------------|--------|--------|
| Claude Code | `~/.claude/projects/<project>/` | JSONL session files | Known — first adapter |
| Codex | TBD | TBD | Post-MVP research |
| OpenCode | TBD | TBD | Post-MVP research |
| Pi | TBD | TBD | Post-MVP research |
| Aider | TBD | TBD | Post-MVP research |

**Strategy:** Claude Code adapter ships first (format is known, most popular harness). The adapter trait is designed so new harnesses can be added without core changes — community PRs welcome. Adapters that can't find their harness's data gracefully no-op.

### Conversation Title Generation

Conversations need meaningful titles in the navigation pane (not raw session IDs):

1. **Prefer agent-provided names** — If the harness already names the session, use it
2. **Heuristic extraction** — If the name is gibberish or a session ID, extract a topic from the first user message and initial assistant response
3. **Future: LLM summarization** — Optional lightweight LLM call (or via the harness itself) to generate better titles. Opt-in, post-MVP.

## Competitive Landscape

| Product | Open Source | Cross-Platform | AI-Aware | Session History | GPU Terminal |
|---------|-----------|----------------|----------|-----------------|-------------|
| **Veil** | Yes | macOS/Linux/Windows | Yes | Yes | Yes (libghosty) |
| cmux | No | macOS only | Yes | No | Yes (libghosty) |
| tmux | Yes | macOS/Linux | No | No | No (TUI) |
| Zellij | Yes | macOS/Linux | No | No | No (TUI) |
| WezTerm | Yes | macOS/Linux/Windows | No | No | Yes (custom) |
| Ghostty | Yes | macOS/Linux | No | No | Yes (libghosty) |
| Claude Desktop | No | macOS/Windows | Yes | Yes | N/A (not a terminal) |

## Distribution

- **MVP:** Package managers — Homebrew (macOS), apt/dnf (Linux), winget/scoop (Windows), `cargo install`
- **Post-MVP:** Pre-built binaries via GitHub Releases
- **Future:** Auto-update mechanism with "update available" notification that directs user to their install method

## License

Dual-licensed under MIT and Apache-2.0 (at user's option). Matches Rust ecosystem convention and maximizes usability.

## Success Criteria

- A developer can launch Veil, open multiple workspaces with AI agents, and navigate between them
- The Conversations tab shows session history from at least Claude Code, grouped and browsable
- The same binary/app runs on macOS, Linux, and Windows
- Terminal rendering quality matches Ghostty (since it uses the same engine)
- Workspace state persists across restarts (when configured)
- Conversations display meaningful titles and progressively accumulate metadata (branch, PR, plan)

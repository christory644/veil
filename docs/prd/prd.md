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
   - **Conversations tab** — Session history browser grouped by agent harness, with titles/previews and timestamps
4. **Cross-platform** — macOS, Linux, Windows (native + WSL)
5. **Keyboard-driven navigation** — Fast switching between workspaces, panes, and conversation history

### P1 — Agent Integration

6. **Session data aggregation** — Read session history from multiple agent harnesses:
   - Claude Code (`~/.claude/projects/<project>/`)
   - Codex (data store TBD — needs research)
   - OpenCode (data store TBD — needs research)
   - Pi (data store TBD — needs research)
7. **Socket API** — JSON-RPC over Unix domain socket (named pipes on Windows) for programmatic control by agents and extensions
8. **Notification system** — Visual notifications from agents via OSC escape sequences and socket API
9. **Session resume** — Jump to a workspace running an agent and resume a previous conversation

### P2 — Polish

10. **Configuration system** — TOML/JSON config for themes, keybindings, sidebar preferences
11. **Ghostty config compatibility** — Read existing Ghostty font/color/theme settings
12. **Shell integration** — Directory tracking, command detection, environment awareness

## Non-Goals

- **Not an IDE** — No built-in editor, file tree, or language server integration
- **Not an AI agent** — Veil orchestrates and navigates agent sessions, it doesn't run its own AI
- **Not a tmux replacement for servers** — Veil is a desktop GUI application, not a headless multiplexer
- **Not a browser** — Unlike cmux, we're not shipping an embedded browser (at least not in MVP)

## Agent Harness Integrations

Each agent harness stores conversation/session data differently. Veil needs adapters for each:

| Agent Harness | Session Data Location | Format | Status |
|---------------|----------------------|--------|--------|
| Claude Code | `~/.claude/projects/<project>/` | JSONL session files | Known — needs mapping |
| Codex | TBD | TBD | Needs research |
| OpenCode | TBD | TBD | Needs research |
| Pi | TBD | TBD | Needs research |
| Aider | TBD | TBD | Needs research |

Building a pluggable adapter system so new harnesses can be added without core changes.

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

## Success Criteria

- A developer can launch Veil, open multiple workspaces with AI agents, and navigate between them
- The Conversations tab shows session history from at least Claude Code, grouped and browsable
- The same binary/app runs on macOS, Linux, and Windows
- Terminal rendering quality matches Ghostty (since it uses the same engine)

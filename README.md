# Veil

Cross-platform, GPU-accelerated terminal workspace manager built on [libghosty](https://github.com/ghostty-org/ghostty), purpose-built for developers working with AI coding agents.

## What is Veil?

Veil is an open-source alternative to [cmux](https://github.com/manaflow-ai/cmux) that runs on macOS, Linux, and Windows. It combines workspace management with AI agent conversation session history in a single tabbed navigation pane — letting you browse, search, and jump between past sessions with Claude Code, Codex, OpenCode, and other agent harnesses.

### Why?

Developers working with AI coding agents juggle multiple projects, multiple agents, and dozens of conversations. Today your options are:

- **cmux** — closest to what we want, but closed-source and macOS-only
- **tmux/zellij** — no AI awareness, no session history
- **Claude Desktop** — great conversation history, but it's not a terminal

Veil bridges this gap: a real terminal workspace manager that also understands your AI agent sessions.

## Features

- **Terminal emulation via libghosty** — GPU-accelerated, Ghostty-quality rendering
- **Workspace management** — Multiple workspaces with configurable split panes
- **Tabbed navigation pane:**
  - **Workspaces** — git branch, working directory, listening ports, PR status, notifications
  - **Conversations** — session history from AI agents, grouped by harness, with meaningful titles
- **Cross-platform** — macOS, Linux, Windows (native + WSL)
- **Keyboard-driven** — every action reachable without a mouse
- **Socket API** — JSON-RPC interface for programmatic control by agents and extensions
- **Ghostty config compatibility** — reads existing font/color/theme settings

## Tech Stack

| Layer | Choice | Why |
|-------|--------|-----|
| Language | Rust | FFI with C (libghosty), cross-platform, performance |
| Terminal | libghosty | Battle-tested VT implementation from Ghostty |
| GPU Rendering | wgpu | Metal/Vulkan/DX12/OpenGL abstraction |
| Windowing | winit | Cross-platform window + event handling |
| Sidebar UI | egui | Immediate-mode GPU-rendered UI |
| Config | TOML | Human-readable, Rust-native |

## Status

**Pre-implementation / Design phase.** We're finalizing architecture decisions and building out the implementation plan.

Design documentation:
- [Product Requirements](docs/prd/prd.md)
- [UI Design](docs/ui_design/ui_design.md)
- [System Design](docs/system_design/system_design.md)

## Agent Harness Support

Veil reads session history from multiple AI coding agents via a pluggable adapter system:

| Agent | Status |
|-------|--------|
| Claude Code | Planned (data format known) |
| Codex | Planned (needs research) |
| OpenCode | Planned (needs research) |
| Pi | Planned (needs research) |
| Aider | Planned (needs research) |

## Building

> Coming soon — Rust workspace setup instructions will be added once implementation begins.

## License

TBD

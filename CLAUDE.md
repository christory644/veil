# Veil

Cross-platform, GPU-accelerated terminal workspace manager built on libghosty, purpose-built for developers working with AI coding agents.

**Status:** Pre-implementation / Design phase

## What is this?

Veil aims to be an open-source alternative to cmux that runs on macOS, Linux, and Windows. Its key differentiator is a tabbed navigation pane with both workspace management and AI agent conversation session history — letting you browse, search, and jump between past sessions with Claude Code, Codex, OpenCode, and other agent harnesses.

## Tech Stack

- **Language:** Rust
- **Terminal engine:** libghosty (from Ghostty, via C FFI)
- **GPU rendering:** wgpu (Metal/Vulkan/DX12/OpenGL)
- **Sidebar UI:** egui (immediate-mode, GPU-rendered)
- **Windowing:** winit

## Documentation

All design docs live in `docs/`:

- **[PRD](docs/prd/prd.md)** — Product requirements, feature priorities, competitive landscape
- **[UI Design](docs/ui_design/ui_design.md)** — Layout, navigation pane design, wireframes, keyboard shortcuts
- **[System Design](docs/system_design/system_design.md)** — Architecture, libghosty integration, cross-platform strategy, session aggregator, socket API

Read these before starting implementation work. They capture the full design context.

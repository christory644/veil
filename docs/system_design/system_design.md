# Veil — System Design Document

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   Veil Application                   │
├───────────┬─────────────────────┬───────────────────┤
│ Navigation│   Terminal Surfaces  │  Session          │
│ Pane UI   │   (libghosty)       │  Aggregator       │
│ (egui)    │                     │                   │
├───────────┴─────────────────────┴───────────────────┤
│              Rendering Layer (wgpu)                   │
│         Metal / Vulkan / DX12 / OpenGL               │
├──────────────────────────────────────────────────────┤
│              Platform Layer                           │
│    winit (windowing) + platform PTY + OS integration │
├──────────────────────────────────────────────────────┤
│         macOS        │    Linux     │    Windows      │
└──────────────────────┴──────────────┴────────────────┘
```

### Core Components

| Component | Responsibility | Key Dependencies |
|-----------|---------------|------------------|
| **App Shell** | Window management, event loop, keyboard dispatch | winit, wgpu |
| **Terminal Engine** | VT parsing, terminal state, render state | libghosty (C FFI) |
| **Renderer** | GPU rendering of terminals and UI | wgpu, libghosty render state |
| **Navigation Pane** | Sidebar UI with tabbed Workspaces/Conversations views | egui |
| **Workspace Manager** | Workspace lifecycle, pane splits, focus tracking | Internal |
| **Session Aggregator** | Reads/indexes session data from agent harnesses | Per-harness adapters |
| **Socket API** | JSON-RPC server for external control | Unix socket / named pipes |
| **Config System** | Settings, keybindings, theming | TOML parser |

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| **Language** | Rust | Strong FFI with C (libghosty), cross-platform, performance, ecosystem (alacritty, wezterm, rio all Rust) |
| **Terminal** | libghosty | Battle-tested VT implementation from Ghostty. C API callable via Rust FFI. Handles parsing, state, render state. |
| **GPU Rendering** | wgpu | Abstracts Metal (macOS), Vulkan (Linux), DX12 (Windows), OpenGL (fallback). Single rendering codebase. |
| **Windowing** | winit | Cross-platform window creation and event handling. Proven in Rust GPU apps. |
| **Sidebar UI** | egui | Immediate-mode GPU-rendered UI. Runs on wgpu. Good for lists, tabs, text. Cross-platform. |
| **Config** | TOML | Familiar to Rust developers, human-readable, well-supported. |

## libghosty Integration

### What libghosty provides

- VT escape sequence parsing (CSI, ESC, DCS, OSC, APC)
- Terminal state management (cursor, styles, scrollback, alt screen)
- Render state API (incremental updates optimized for GPU rendering)
- Input encoding (keyboard/mouse → escape sequences, Kitty protocol)
- Content access (cells, graphemes, serialization)
- Kitty Graphics Protocol support

### What we build on top

- PTY management (fork/exec, process lifecycle)
- GPU rendering pipeline (consuming libghosty render state → wgpu draw calls)
- Font rasterization and glyph atlas
- All GUI elements (sidebar, tabs, splits, notifications)
- Shell integration hooks
- Configuration and theming

### FFI Approach

libghosty exposes a C API. Rust consumes it via:

```
libghosty (Zig → C ABI)
    ↓
rust-bindgen (auto-generated Rust bindings from C headers)
    ↓
veil-ghostty crate (safe Rust wrapper with idiomatic API)
    ↓
Veil application code
```

The `veil-ghostty` crate provides a safe abstraction over the raw C API, handling:
- Lifetime management for terminal instances
- Callback registration for render state updates
- Input event translation
- Memory safety around C pointers

### Build Integration

libghosty is built from source (Zig) and linked as a static library:
- Build via Zig build system, invoked from Rust's `build.rs`
- Produces platform-specific static lib (`.a` on macOS/Linux, `.lib` on Windows)
- Linked into final binary — no runtime dependency

Reference: Ghostty's own macOS app uses this pattern (Zig → XCFramework → Swift). The Ghostling project demonstrates minimal C embedding.

## Cross-Platform Rendering

### wgpu Backend Selection

| Platform | Primary Backend | Fallback |
|----------|----------------|----------|
| macOS | Metal | OpenGL |
| Linux | Vulkan | OpenGL |
| Windows | DX12 | Vulkan → OpenGL |

wgpu handles backend selection automatically. The rendering code is identical across platforms.

### Render Pipeline

```
Terminal State (libghosty)
    ↓ render state diff
Glyph Atlas (rasterized font glyphs → GPU texture)
    ↓
Vertex Buffer (cell positions + glyph UV coords + colors)
    ↓
wgpu Render Pass
    ├── Terminal background (solid color quads)
    ├── Text (textured quads from glyph atlas)
    ├── Cursor (animated quad)
    ├── Selection highlight (semi-transparent overlay)
    └── Sidebar UI (egui render pass)
    ↓
Swapchain Present
```

### Font Rendering

- Use platform font APIs for discovery (CoreText on macOS, fontconfig on Linux, DirectWrite on Windows)
- Rasterize glyphs to texture atlas on CPU
- Upload atlas to GPU, render text as textured quads
- Reference: wezterm and rio both solve this problem in Rust — study their approaches

## PTY Management

### Per-Platform

| Platform | PTY API | Process Spawning |
|----------|---------|-----------------|
| macOS | `posix_openpt` / `forkpty` | `fork` + `exec` |
| Linux | `posix_openpt` / `forkpty` | `fork` + `exec` |
| Windows | ConPTY (`CreatePseudoConsole`) | `CreateProcess` |

### Architecture

Each terminal surface owns:
- A PTY master/slave pair
- A child process (shell or agent command)
- A read thread (PTY master → libghosty parser)
- A write path (keyboard input → PTY master)

Workspace → contains N panes → each pane has 1 surface → each surface has 1 PTY + 1 process.

## Session Aggregator

The session aggregator reads conversation history from agent harness data stores and presents it in the Conversations tab.

### Architecture

```
┌─────────────────────────────────┐
│       Session Aggregator         │
├─────────────────────────────────┤
│  ┌───────────┐ ┌──────────────┐ │
│  │ Adapter:  │ │ Adapter:     │ │
│  │ Claude    │ │ Codex        │ │
│  │ Code      │ │              │ │
│  └───────────┘ └──────────────┘ │
│  ┌───────────┐ ┌──────────────┐ │
│  │ Adapter:  │ │ Adapter:     │ │
│  │ OpenCode  │ │ Pi           │ │
│  └───────────┘ └──────────────┘ │
├─────────────────────────────────┤
│     Unified Session Index        │
│  (sorted, searchable, cached)    │
└─────────────────────────────────┘
```

### Adapter Interface

Each agent harness adapter implements a common trait:

```rust
trait AgentAdapter {
    /// Human-readable name for this harness
    fn name(&self) -> &str;

    /// Discover all session files/data for this harness
    fn discover_sessions(&self) -> Vec<SessionEntry>;

    /// Get detailed content/preview for a specific session
    fn session_preview(&self, id: &SessionId) -> Option<SessionPreview>;
}
```

### Known Session Data Locations

**Claude Code:**
- Location: `~/.claude/projects/<project-hash>/`
- Format: JSONL files containing conversation turns
- Contains: User messages, assistant responses, tool calls, timestamps
- Title extraction: First user message or auto-summary

**Codex / OpenCode / Pi / Aider:**
- Locations and formats TBD — needs research per harness
- Each gets its own adapter implementation
- Adapters that can't find their harness's data gracefully no-op

### Indexing Strategy

- On startup: scan known locations, build in-memory index
- File watcher: monitor session directories for new/changed files
- Cache: lightweight SQLite or in-memory cache for session metadata
- Lazy loading: only read full session content when user previews/selects

## Socket API

### Transport

| Platform | Transport |
|----------|-----------|
| macOS/Linux | Unix domain socket (`/tmp/veil.sock` or `$XDG_RUNTIME_DIR/veil.sock`) |
| Windows | Named pipe (`\\.\pipe\veil`) |

### Protocol

JSON-RPC 2.0 over newline-delimited JSON:

```json
{"jsonrpc":"2.0","id":1,"method":"workspace.list","params":{}}
{"jsonrpc":"2.0","id":1,"result":[{"id":"ws-1","name":"api-server","branch":"main"}]}
```

### Core Methods

**Workspace:**
- `workspace.create` / `workspace.list` / `workspace.select` / `workspace.close` / `workspace.rename`

**Surface/Pane:**
- `surface.split` / `surface.focus` / `surface.list` / `surface.send_text`

**Notifications:**
- `notification.create` / `notification.list` / `notification.clear`

**Sidebar:**
- `sidebar.set_status` / `sidebar.set_progress`

**Sessions:**
- `session.list` / `session.search` / `session.preview`

### Environment Variables

Veil sets these in child processes:
- `VEIL_WORKSPACE_ID` — current workspace ID
- `VEIL_SURFACE_ID` — current pane/surface ID
- `VEIL_SOCKET` — path to socket
- `TERM_PROGRAM=ghostty` — compatibility with Ghostty-aware tools
- `TERM=xterm-ghostty`

## Configuration

### File Location

- `~/.config/veil/config.toml` (primary, all platforms)
- `~/Library/Application Support/com.veil.app/config.toml` (macOS alternate)
- `%APPDATA%\veil\config.toml` (Windows alternate)

### Structure

```toml
[general]
theme = "dark"  # "dark", "light", "system"

[sidebar]
default_tab = "workspaces"  # "workspaces" or "conversations"
width = 250  # pixels
visible = true

[terminal]
scrollback_lines = 10000

[conversations]
# Agent harness adapters to enable
adapters = ["claude-code", "codex", "opencode"]

[keybindings]
workspace_tab = "ctrl+shift+w"
conversations_tab = "ctrl+shift+c"
new_workspace = "ctrl+shift+n"

[ghostty]
# Path to Ghostty config for font/color/theme import
config_path = "~/.config/ghostty/config"
```

## Key References

| Resource | Relevance |
|----------|-----------|
| [Ghostty](https://github.com/ghostty-org/ghostty) | Source of libghosty, reference for macOS apprt FFI |
| [Ghostling](https://github.com/ghostty-org/ghostling) | Minimal libghosty embedding example (C + Raylib) |
| [wezterm](https://github.com/wezterm/wezterm) | Rust terminal with GPU rendering, cross-platform PTY, font handling |
| [rio](https://github.com/niconiahi/rio) | Rust terminal with wgpu rendering |
| [alacritty](https://github.com/alacritty/alacritty) | Rust terminal, cross-platform, performance reference |
| [egui](https://github.com/emilk/egui) | Immediate-mode GUI for Rust, runs on wgpu |
| [cmux](https://github.com/manaflow-ai/cmux) | Closed-source reference for workspace UX, socket API patterns |

## Caveats & Risks

1. **libghosty API instability** — The C API is functional but signatures are still in flux. We'll need to track upstream and update bindings periodically. Pin to a specific Ghostty commit for stability.
2. **Font rendering complexity** — Cross-platform font discovery and rasterization is a significant effort. Consider using an existing crate (e.g., `cosmic-text`, `swash`) rather than building from scratch.
3. **Windows ConPTY** — Windows pseudo-terminal support has quirks. wezterm's implementation is a good reference.
4. **Agent data format changes** — Agent harnesses may change their session data formats. Adapters need to be resilient and version-aware.
5. **GPU driver compatibility** — wgpu helps but edge cases exist, especially with older Intel GPUs on Linux. OpenGL fallback is important.

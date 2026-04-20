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
| **Font Rendering** | swash + rustybuzz | swash for glyph rasterization, rustybuzz for OpenType shaping (ligatures). Lighter than cosmic-text since we only need monospace grid rendering. |
| **Session Cache** | SQLite (rusqlite) | Persistent session metadata cache. Survives restarts, handles growing history, enables full-text search on conversation titles. |
| **Observability** | tracing | Structured, async-aware, leveled logging. Zero-cost when disabled. |
| **Config** | TOML | Familiar to Rust developers, human-readable, well-supported. |
| **Testing** | proptest + criterion | Property-based testing for parsing/state, criterion for performance benchmarks. Standard #[test] for units. |

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

See the dedicated [Font Rendering](#font-rendering) section below for full details. Summary:
- Platform font APIs for discovery (CoreText / fontconfig / DirectWrite)
- rustybuzz for OpenType shaping (ligatures)
- swash for glyph rasterization
- Dynamic glyph atlas uploaded to GPU
- Fallback font chain for Nerd Font glyphs and emoji

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

- On startup: scan known locations, populate SQLite cache
- File watcher (notify crate): monitor session directories for new/changed files
- SQLite: persistent cache for session metadata, titles, branch/PR associations
- Lazy loading: only read full session content when user previews/selects
- Full-text search: SQLite FTS5 on conversation titles and first messages

### Progressive Conversation Metadata

Conversation DB records start minimal and enrich over time as agent activity is observed:

```
Session detected   → (id, agent, working_dir, timestamp)
Title available    → + title (from agent or heuristic extraction)
Agent branches     → + branch_name
Agent opens PR     → + pr_number, pr_url
Agent plans        → + plan_reference (finalized plan content)
Session ends       → + end_timestamp, exit_status
```

**Detection methods:**
- **From session data** (preferred): Claude Code JSONL includes tool calls — detect git operations, PR creation directly from structured data
- **From PTY observation** (fallback): Pattern match on terminal output for `git checkout -b`, `gh pr create` output, etc.
- **Git state correlation**: Map working directory + timestamp to git log to determine active branch at session start

### Live State Awareness

Cached metadata is the *historical* association. At render time, cross-reference with current state:

| Cached Data | Live Check | Display |
|-------------|-----------|---------|
| Branch X | `git branch --list X` | Show normally, or dimmed + "(deleted)" if gone |
| PR #123 | `gh pr view 123 --json state` | Badge showing current status (open/merged/closed) |
| Working dir | `Path::exists()` | Warning if directory no longer exists |

Live checks are cached with short TTL (30s–60s) to avoid constant git/GitHub API calls.

### Title Generation

1. **Agent-provided name**: Use if available and not a raw session ID / gibberish
2. **Heuristic extraction**: Parse first user message + initial assistant response, extract a topic phrase
3. **Detection of gibberish**: If title matches UUID/hash pattern or is purely numeric, fall back to heuristic
4. **Future (post-MVP)**: Optional lightweight LLM summarization, or invoke the harness itself to name the session

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

## State Management

Veil uses a **hybrid architecture**: centralized state for the UI layer, actor model for I/O subsystems.

```
┌─────────────────────────────────────────┐
│           UI Thread (egui)              │
│   Reads from AppState each frame        │
│   Emits user events (clicks, keys)      │
├─────────────────────────────────────────┤
│           Central AppState              │
│   Updated via message channel (mpsc)    │
│   Single source of truth for rendering  │
├─────────────────────────────────────────┤
│   Background Actors (own internal       │
│   state, push updates to AppState):     │
│   - PTY Manager (N read loops)          │
│   - Session Aggregator (file watcher)   │
│   - Socket API Server                   │
│   - Config Watcher                      │
├─────────────────────────────────────────┤
│   Command channels flow downward:       │
│   UI events → Actor command channels    │
└─────────────────────────────────────────┘
```

### Rationale

- **UI needs centralized state**: egui is immediate-mode — every frame it reads state and draws. A coherent `AppState` struct is required.
- **I/O is naturally concurrent**: N PTY processes, file watchers, socket connections, config watcher all run independently and shouldn't block the UI.
- **Message passing connects them**: Actors send `StateUpdate` messages to the central store via channels. User events flow to actors via command channels.

### AppState Structure

```rust
struct AppState {
    workspaces: Vec<Workspace>,
    active_workspace: WorkspaceId,
    conversations: ConversationIndex,
    config: AppConfig,
    notifications: Vec<Notification>,
    sidebar: SidebarState,
}
```

### Debug Overlay

Development builds include a debug overlay (gated behind `--debug` flag or `debug-overlay` feature flag) that renders:
- Full AppState tree
- Active actor status and message throughput
- Frame time graph
- Channel queue depths
- SQLite query stats

Built with egui — same rendering primitives as the sidebar.

## Error Handling

**Philosophy: transparent, informative, user-in-control.** Like Rust compiler errors — structured, contextual, actionable.

### Principles

1. **Never silently swallow errors** — Every failure surfaces to the user in context
2. **Never auto-dismiss** — User acknowledges errors on their own timeline
3. **Never crash the whole app** — Isolate failures to their component
4. **Be informative** — Show what happened, why, and what options the user has

### Per-Component Strategy

| Component | Failure Mode | Behavior |
|-----------|-------------|----------|
| PTY/Surface | Process exits | Shell handles naturally (user sees exit code). Veil shows exit status in pane if non-zero. |
| PTY/Surface | PTY I/O error | Structured error message displayed in-pane. User can close or retry. |
| libghosty | FFI panic | `catch_unwind` at FFI boundary. Affected surface shows error. Other surfaces unaffected. |
| Adapter | Can't read session data | Skip with warning in sidebar. Log structured error. Rest of conversations tab works. |
| Config | Parse error | Show error with line number. Continue with previous valid config. |
| Socket API | Client error | Error response per JSON-RPC spec. Server stays up. |
| SQLite | DB corruption | Rebuild from source session files. Notify user. |

### Error Display

Errors render in-context (in the pane where they occurred, or in the sidebar if sidebar-related) with:
- **What happened** — Clear description
- **Why** — Technical detail (expandable)
- **What you can do** — Actionable options (close, retry, report)

## Observability

### Logging (tracing crate)

- **Structured spans and events** — async-aware, zero-cost when disabled
- **Performance constraint** — Minimize overhead in hot paths (render loop, PTY read). Use `tracing::instrument` selectively.
- **Output targets:**
  - stderr: Human-readable (development)
  - File: Structured JSON (`~/.local/share/veil/logs/`) for debugging user-reported issues
  - Debug overlay: Live feed in dev builds
- **Crash safety** — ERROR and WARN levels use unbuffered writes. Panic and signal hooks flush all buffers before exit.

### Log Levels

| Level | Usage |
|-------|-------|
| ERROR | Unrecoverable component failure (shown to user) |
| WARN | Degraded functionality (adapter skip, fallback triggered) |
| INFO | Lifecycle events (workspace created, session indexed) |
| DEBUG | State transitions, message flow |
| TRACE | Per-frame, per-event detail (off in release) |

## Testing Strategy

Testing is a core pillar — the test suite serves as guardrails for agentic development, ensuring AI agents can build features with confidence.

### Test Pyramid

| Layer | Tool | Scope |
|-------|------|-------|
| **Unit** | `#[test]` + mockall | Individual functions, adapter parsing, state transitions. Mock boundaries. Happy + unhappy paths. |
| **Property-based** | proptest | VT parsing edge cases, state machine invariants, adapter input fuzzing. |
| **Integration** | `#[test]` (feature-gated) | Real libghosty, real file system, real SQLite. Tests with actual dependencies. |
| **Performance** | criterion | Frame time budgets, session indexing speed, startup time, glyph atlas operations. |
| **E2E** | Socket API driven | Full app launched, driven programmatically via JSON-RPC socket. Asserts on state via API responses. No GUI automation needed. |

### CI

- **GitHub Actions** on all three platforms (macOS, Linux, Windows)
- **Blocks merges** on test failure
- **Matrix:** debug + release builds, all backends
- Test categories gated by CI environment (integration tests need libghosty built, E2E needs display server)

### TDD Workflow

Tests are written first. Implementation follows to make them pass. This is enforced by convention and review, not tooling.

## Configuration Hot-Reload

Config changes apply live without restart:

- File watcher (notify crate) on config file
- On change: parse new config, diff against current, apply deltas
- **Live-reloadable:** font, theme, colors, keybindings, sidebar width/visibility, conversation adapter settings
- **Requires restart:** GPU backend (rare — only if user manually forces a different backend)
- Invalid config: keep previous valid state, show parse error to user

## Font Rendering

### Requirements

- Monospace grid rendering with **ligature support** (Fira Code, JetBrains Mono, etc.)
- **Nerd Font / glyph support** (powerline, devicons, emoji)
- User-configurable font (Veil config or imported from Ghostty config)
- Fallback font chain for missing glyphs

### Architecture

```
Font Config (user-specified or Ghostty import)
    ↓
Platform Font Discovery (CoreText / fontconfig / DirectWrite)
    ↓
rustybuzz (OpenType shaping → ligatures, glyph substitution)
    ↓
swash (glyph rasterization → bitmap)
    ↓
Glyph Atlas (GPU texture, dynamic growth)
    ↓
Render as textured quads in wgpu pipeline
```

### Fallback Chain

1. User-configured primary font
2. Nerd Font variant (if available)
3. Platform default monospace
4. Built-in fallback for box-drawing and powerline glyphs

Reference implementations: alacritty, wezterm, and rio all solve this in Rust.

## Workspace Persistence

### Behavior

Configurable — user chooses via config or first-run exit dialog:
- **Restore mode**: On launch, restore previous workspaces (directories, splits, layout). Does not restore shell history or running processes.
- **Fresh mode**: Always start with empty state.
- **Ask mode** (default on first run): On exit, prompt "Restore this session next time?" with option to save preference.

### Serialization

Workspace state serialized to `~/.local/share/veil/state.json` (or platform equivalent) on exit:
```rust
struct PersistedState {
    workspaces: Vec<PersistedWorkspace>,
    active_workspace: WorkspaceId,
    sidebar_state: SidebarPersistence,
}

struct PersistedWorkspace {
    name: String,
    working_directory: PathBuf,
    layout: PaneLayout,  // tree of splits
}
```

## Caveats & Risks

1. **libghosty API instability** — The C API is functional but signatures are still in flux. Pin to a specific Ghostty commit. Update bindings periodically.
2. **Font rendering complexity** — Cross-platform font discovery and rasterization is significant. swash + rustybuzz chosen over cosmic-text for lighter weight. Reference wezterm/rio implementations.
3. **Windows ConPTY** — Pseudo-terminal support has quirks. wezterm's implementation is a good reference.
4. **Agent data format changes** — Harnesses may change session data formats. Adapters need to be resilient and version-aware.
5. **GPU driver compatibility** — wgpu helps but edge cases exist (older Intel GPUs on Linux). OpenGL fallback is important.
6. **Session data growth** — SQLite cache + lazy loading mitigates, but long-term users with thousands of sessions need pagination and efficient querying.
7. **Cross-tab state consistency** — Workspace and conversation views reference overlapping data. Central AppState ensures consistency but message ordering matters.

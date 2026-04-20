# Veil — UI Design Document

## Layout Overview

Veil's UI consists of three main regions:

```
┌──────────────┬──────────────────────────────────────────┐
│              │                                          │
│  Navigation  │                                          │
│    Pane      │            Terminal Area                  │
│              │         (split panes)                     │
│  [Tabs]      │                                          │
│  ┌────┬────┐ │                                          │
│  │ WS │Conv│ │                                          │
│  └────┴────┘ │                                          │
│              │                                          │
│  (tab        │                                          │
│   content)   │                                          │
│              │                                          │
│              ├──────────────────┬───────────────────────┤
│              │                  │                       │
│              │   Pane 2         │    Pane 3             │
│              │                  │                       │
└──────────────┴──────────────────┴───────────────────────┘
```

1. **Navigation Pane** (left) — Fixed-width sidebar with tabbed views
2. **Terminal Area** (right) — Workspace content with configurable split panes

## Navigation Pane

The navigation pane is a single sidebar with **two tabs** at the top:

### Tab 1: Workspaces (`WS`)

Lists all open workspaces. Each entry shows contextual metadata:

```
┌──────────────┐
│ [WS] [Conv]  │
├──────────────┤
│              │
│ ● api-server │
│   main       │
│   ~/repos/api│
│   :3000 :5432│
│              │
│ ○ client-app │
│   feat/auth  │
│   ~/repos/web│
│   PR #142    │
│              │
│ ○ infra      │
│   main       │
│   ~/repos/iac│
│              │
│ ○ scratch    │
│   (no git)   │
│   ~/tmp      │
│              │
└──────────────┘
```

**Workspace entry fields:**
- **Name** — User-defined or auto-detected from directory
- **Git branch** — Current branch (if applicable)
- **Working directory** — Abbreviated path
- **Listening ports** — Detected open ports
- **PR status** — Linked PR number/status (if detected)
- **Notification badge** — Unread notification indicator
- **Active indicator** — `●` for focused, `○` for background

**Interactions:**
- Click or `Cmd+1-9` to switch workspaces
- Right-click for context menu (rename, close, move)
- Drag to reorder
- `Cmd+N` to create new workspace

### Tab 2: Conversations (`Conv`)

Displays conversation session history from AI agent harnesses, grouped by harness:

```
┌──────────────┐
│ [WS] [Conv]  │
├──────────────┤
│              │
│ ▼ Claude Code│
│              │
│  "Fix auth   │
│   middleware" │
│   api-server │
│   2h ago     │
│              │
│  "Add user   │
│   migration" │
│   api-server │
│   yesterday  │
│              │
│  "Debug CI   │
│   pipeline"  │
│   infra      │
│   2 days ago │
│              │
│ ▶ Codex (3)  │
│              │
│ ▶ OpenCode(1)│
│              │
└──────────────┘
```

**Conversation entry fields:**
- **Title/Preview** — First message summary or auto-generated title
- **Associated workspace/project** — Which project directory this session belongs to
- **Timestamp** — Relative time (2h ago, yesterday, etc.)
- **Status** — Active, completed, or interrupted

**Group headers:**
- Agent harness name with count of sessions
- Collapsible (`▼` expanded, `▶` collapsed)
- Sorted by most recent activity within each group

**Interactions:**
- Click to navigate to the workspace where this session ran (or offer to open one)
- Search/filter across all conversations (`/` to focus search)
- Scroll through history (lazy-loaded, most recent first)
- Keyboard: `j/k` or arrow keys to navigate entries, `Enter` to select

## Tab Switching

- **Mouse**: Click tab headers
- **Keyboard**: `Ctrl+Shift+W` for Workspaces, `Ctrl+Shift+C` for Conversations (configurable)
- Active tab is visually highlighted (underline or background color)

## Terminal Area

The main content area where terminal panes live.

### Split Panes

- **Horizontal split**: `Cmd+D` (side by side)
- **Vertical split**: `Cmd+Shift+D` (top and bottom)
- **Navigate panes**: `Cmd+[` / `Cmd+]` or `Ctrl+hjkl`
- **Resize panes**: `Cmd+Ctrl+Arrow` or drag dividers
- **Close pane**: `Cmd+W`
- **Zoom pane**: `Cmd+Shift+Enter` (toggle fullscreen for focused pane)

### Pane Chrome

Minimal — thin divider lines between panes. Focused pane gets a subtle border highlight. No per-pane title bars by default (configurable).

## Notification System

Agents can send notifications via:
- OSC 9/99/777 escape sequences (terminal standard)
- Socket API (`notification.create`)

Notifications appear as:
- Badge on the workspace entry in the Workspaces tab
- Visual ring/highlight on the pane border
- Optional desktop notification (configurable)
- Latest notification text shown as subtitle on workspace entry

## Theming & Appearance

- Reads Ghostty config for terminal font, colors, and themes
- Sidebar/chrome theming via Veil's own config (light/dark, accent colors)
- Respects system dark mode preference
- GPU-accelerated rendering throughout (terminal and sidebar)

## Keyboard Navigation Summary

| Action | Shortcut |
|--------|----------|
| Switch workspace | `Cmd+1-9` |
| New workspace | `Cmd+N` |
| Workspaces tab | `Ctrl+Shift+W` |
| Conversations tab | `Ctrl+Shift+C` |
| Split horizontal | `Cmd+D` |
| Split vertical | `Cmd+Shift+D` |
| Navigate panes | `Cmd+[` / `Cmd+]` |
| Close pane | `Cmd+W` |
| Zoom pane | `Cmd+Shift+Enter` |
| Search conversations | `/` (in Conversations tab) |
| Toggle sidebar | `Cmd+B` |

*Note: All shortcuts are configurable. Linux/Windows variants use `Ctrl` instead of `Cmd`.*

## Design Principles

1. **Terminal first** — The terminal area gets maximum space. Sidebar is compact and hideable.
2. **Keyboard-driven** — Every action is reachable without a mouse.
3. **Progressive disclosure** — Workspace entries show essential info by default, details on hover/focus.
4. **Fast** — Navigation pane renders at GPU speed alongside terminals. No web views, no Electron.
5. **Familiar** — Borrow proven patterns from cmux (workspace sidebar), Claude Desktop (conversation nav), and tmux (keybindings).

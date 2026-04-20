# Veil — UI Design Document

## Layout Overview

Veil's UI consists of two main regions representing two conceptual spaces:

- **Workspaces** = **Userspace** — The user's terminal environment: panes, splits, directories, running processes
- **Conversations** = **Agent space** — All AI agent session history, grouped by harness, with progressive metadata

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
- **Agent indicator** — Small icon/badge if an AI agent is running in one of the workspace's panes

**Interactions:**
- Click or `Cmd+1-9` to switch workspaces
- Right-click for context menu (rename, close, move)
- Drag to reorder
- `Cmd+N` to create new workspace

### Tab 2: Conversations (`Conv`)

Displays conversation session history from AI agent harnesses, grouped by harness. This is the **agent space** view — a temporal lens into all AI sessions, past and present.

```
┌──────────────┐
│ [WS] [Conv]  │
├──────────────┤
│              │
│ ▼ Claude Code│
│   [+]        │
│              │
│  ● "Fix auth │
│   middleware" │
│   feat/auth  │
│   PR #142    │
│   2h ago     │
│              │
│  ○ "Add user │
│   migration" │
│   feat/users │
│   PR #138 ✓  │
│   yesterday  │
│              │
│  ○ "Debug CI │
│   pipeline"  │
│   main       │
│   2 days ago │
│              │
│ ▶ Codex (3)  │
│   [+]        │
│              │
│ ▶ OpenCode(1)│
│   [+]        │
│              │
└──────────────┘
```

**Conversation entry fields:**
- **Title** — Meaningful name (agent-provided or heuristically extracted, never raw session IDs)
- **Branch** — Git branch the conversation was/is associated with
- **PR status** — PR number with live state badge (open, merged ✓, closed)
- **Timestamp** — Relative time (2h ago, yesterday, etc.)
- **Active indicator** — `●` for live/running sessions, `○` for completed/historical
- **Plan indicator** — Icon if a finalized plan is associated with this session

Note: The agent harness identification comes from the group hierarchy (parent header), not repeated on each entry.

**Group headers:**
- Agent harness name with count of sessions
- Collapsible (`▼` expanded, `▶` collapsed)
- Sorted by most recent activity within each group
- `[+]` button to start a new session with that agent

**Interactions:**
- Click active conversation → navigate to the workspace/pane where it's running
- Click historical conversation → show session details (plan, branch, PR, preview) with option to start a new session in same project
- Search/filter across all conversations (`/` to focus search)
- Scroll through history (lazy-loaded, most recent first)
- Keyboard: `j/k` or arrow keys to navigate entries, `Enter` to select
- `[+]` or keybinding to start a new agent session (opens/creates a workspace pane and launches the agent)

**Progressive metadata:**
Conversation entries enrich over time as the agent works. A newly started session shows only title + working directory. As the agent creates branches, opens PRs, or finalizes plans, those appear automatically in the entry without user action.

**Live state awareness:**
Historical metadata is cross-referenced with current state:
- Branch deleted → shown dimmed with "(deleted)"
- PR merged → green merged badge
- PR closed → red closed badge
- Directory no longer exists → warning indicator

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

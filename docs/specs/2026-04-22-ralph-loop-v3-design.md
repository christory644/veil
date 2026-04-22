# Ralph Loop v3: Subagent-Driven Architecture

## Problem

Ralph loop v1 produced 150 commits with poor quality (no TDD, bloated files, standalone doc commits, AGENTS.md journaling). Ralph loop v2 fixed the process rules but the agent completed 5 tasks in a single 99-minute session because "exit cleanly" didn't actually terminate the process, and context rot degraded quality as the session grew to 5MB.

The root cause in both cases: a single agent doing too much work in one context window.

## Solution

Restructure the ralph loop so the main agent is a **lean orchestrator** that delegates all heavy work to focused subagents. Each subagent gets a fresh context window, preventing context rot. The main agent never writes code — it only orchestrates, commits, and manages Linear.

## Pipeline

### Phase 0: Orient

Main agent reads `AGENTS.md`, runs `cargo build`, `cargo test`, `git log --oneline -10`, `git status`. Notes the current HEAD commit hash (needed for coderabbit base later). Commits any clean uncommitted work from a previous iteration. Stashes broken work.

### Phase 1: Select Task

Query Linear for highest-priority unblocked issue in Veil-term team. Move to "In Progress". Leave a comment.

### Phase 2: Plan (subagent)

Spawn a planning subagent that:

1. Reads the Linear issue description and the project's design docs (`docs/prd/prd.md`, `docs/system_design/system_design.md`, `docs/ui_design/ui_design.md`)
2. Reads relevant existing code in the codebase
3. Invokes superpowers `brainstorming` and `writing-plans` skills to produce a structured plan
4. Writes the plan as a spec to `docs/specs/VEI-XX-<slug>.md`

The spec contains:
- **Context**: What this task does and why (from Linear issue)
- **Implementation units**: The decomposition of work into independent units, where each unit maps to a set of related functions/types that can be tested and implemented together
- **Test strategy**: What tests each unit needs (happy path, error cases, edge cases)
- **Acceptance criteria**: What "done" looks like
- **Dependencies**: Any tools or libraries that need to be installed

Main agent commits: `plan(VEI-XX): design spec for <feature>`

### Phase 3: Write Failing Tests (subagent)

Spawn a test-writing subagent that:

1. Reads the spec file
2. Reads existing code to understand the codebase structure
3. Writes ALL tests for ALL implementation units
4. Organizes tests by unit (separate test modules or clearly labeled sections)
5. Ensures tests compile but fail (RED state)

Main agent runs `cargo test` to confirm tests exist and fail for the right reasons (not compilation errors). Main agent commits: `test(VEI-XX): RED for <brief description of units>`

### Phase 4: Implement (one subagent per unit)

For each implementation unit defined in the spec:

1. Spawn an implementation subagent with:
   - The spec file path
   - The specific unit name/description
   - The test file paths for that unit
2. Subagent reads the tests, reads existing code, writes minimal implementation to make those tests pass
3. Main agent runs `cargo test` to verify the unit's tests now pass
4. If tests still fail, main agent can retry with a fresh subagent (up to 2 retries)
5. Main agent commits: `feat(VEI-XX): implement <unit name>`

Units are processed sequentially. Later units may depend on earlier ones.

### Phase 5: Refactor (subagent)

Spawn a refactoring subagent that:

1. Reads all files modified during this task (from `git diff --name-only <phase-0-head>`)
2. Refactors for clarity: better names, reduced complexity, extracted functions
3. Ensures each file has a single cohesive responsibility — splits files that mix concerns into focused modules (the trigger is mixed responsibilities, not line count)
4. Ensures all tests still pass

Main agent runs full quality gate (`cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test && cargo build`). Main agent commits: `refactor(VEI-XX): <what improved>`

### Phase 6: Review (subagent loop)

Each review iteration spawns a NEW subagent (fresh context):

1. Subagent runs: `coderabbit review --plain --type committed --base-commit <phase-0-head>`
2. Evaluates each finding:
   - **Fix**: logic errors, missing edge cases, unsafe misuse, test gaps, resource leaks
   - **Ignore**: formatting, clippy lints (already gated), style preferences
   - When ignoring, note why
3. Fixes legitimate issues
4. Main agent runs quality gate, commits fixes

Loop until coderabbit returns no actionable feedback. Maximum 3 iterations to prevent infinite loops.

### Phase 7: Push

Main agent runs `git push`. Pre-push hooks (see below) enforce the quality gate as a safety net. If hooks deny the push, main agent fixes the issue and retries.

### Phase 8: Update Linear and STOP

1. Move issue to "Done"
2. Leave a summary comment (what was implemented, tests added, refactoring done)
3. **END RESPONSE** — do not start another task

## Hooks

### Pre-push Quality Gate

`.claude/settings.json` configures a PreToolUse hook that intercepts `git push` commands:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "if": "Bash(git push *)",
        "hooks": [
          {
            "type": "command",
            "command": ".claude/hooks/pre-push-gate.sh",
            "timeout": 120
          }
        ]
      }
    ]
  }
}
```

The hook script runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`. Returns a deny decision with error output if any check fails.

## Environment Setup

The prompt includes this principle:

> You have full system access (bypass permissions mode). If a task requires installing tools, cloning repositories, or setting up build dependencies, DO IT. Do not declare a task blocked because a dependency isn't pre-installed. Setting up the environment IS part of the work. If you install something on the host system, document what you installed in the commit message.

This ensures the agent handles VEI-6 (libghosty FFI) which requires installing Zig and cloning the Ghostty source, rather than declaring it impossible.

## Constraints

- **One task per session**: The main agent completes exactly one Linear task then stops.
- **No issue creation**: The backlog is human-curated. The agent does not create new Linear issues.
- **No standalone doc/style/chore commits**: Documentation ships with code. Formatting is handled by the quality gate.
- **Single responsibility per file**: Each file should express one cohesive unit. Extract and refactor to keep responsibilities segregated — no god objects, no mixed concerns. Most files will naturally land around ~300 lines, but a module that genuinely needs 700 lines to express a single unit cleanly is fine. The signal to split is mixed responsibilities, not line count.
- **Commit messages include VEI-XX**: Every commit references the Linear issue number.
- **Host system changes in commit messages**: If the agent installs a tool (e.g., Zig), the commit message documents it.

## Files to Create/Modify

1. **`PROMPT.md`** — Complete rewrite with subagent orchestration pipeline
2. **`.claude/settings.json`** — Hook configuration for pre-push quality gate
3. **`.claude/hooks/pre-push-gate.sh`** — Shell script for quality gate enforcement
4. **`AGENTS.md`** — Minor updates to reference the new pipeline and specs directory

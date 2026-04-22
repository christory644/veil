# Ralph Loop v3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the ralph loop infrastructure so the main agent orchestrates subagents instead of doing all work itself, preventing context rot and enforcing quality gates via hooks.

**Architecture:** The main agent becomes a lean orchestrator (PROMPT.md). Heavy work (planning, testing, implementing, refactoring, reviewing) is delegated to focused subagents with fresh context windows. A pre-push hook (.claude/hooks/pre-push-gate.sh) enforces the quality gate as a safety net. Specs are committed per task as auditable artifacts.

**Tech Stack:** Bash (hooks), Claude Code settings.json (hook config), Markdown (prompt/agents docs)

---

### Task 1: Create pre-push quality gate hook

**Files:**
- Create: `.claude/settings.json`
- Create: `.claude/hooks/pre-push-gate.sh`

- [ ] **Step 1: Create `.claude/settings.json` with hook config**

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

- [ ] **Step 2: Create `.claude/hooks/pre-push-gate.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Pre-push quality gate hook for Claude Code
# Blocks git push unless cargo fmt, clippy, and tests all pass.
# Called by Claude Code PreToolUse hook — receives tool input on stdin.

cd "${CLAUDE_PROJECT_DIR:-.}"

# Check formatting
if ! cargo fmt --check 2>&1; then
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"cargo fmt --check failed. Run: cargo fmt"}}' >&2
    exit 2
fi

# Check clippy
if ! cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"cargo clippy failed. Fix warnings before pushing."}}' >&2
    exit 2
fi

# Check tests
if ! cargo test 2>&1; then
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"cargo test failed. Fix failing tests before pushing."}}' >&2
    exit 2
fi

exit 0
```

- [ ] **Step 3: Make the hook script executable**

Run: `chmod +x .claude/hooks/pre-push-gate.sh`

- [ ] **Step 4: Verify the hook script runs correctly in isolation**

Run: `echo '{"tool_name":"Bash","tool_input":{"command":"git push origin ralph-loop"}}' | .claude/hooks/pre-push-gate.sh; echo "exit: $?"`
Expected: Script runs cargo checks (will fail since no Cargo.toml yet on this branch — that's fine, confirms it's wired up)

- [ ] **Step 5: Commit**

```bash
git add .claude/settings.json .claude/hooks/pre-push-gate.sh
git commit -m "feat: add pre-push quality gate hook for Claude Code

Blocks git push unless cargo fmt --check, clippy -D warnings, and
cargo test all pass. Enforced by Claude Code PreToolUse hook so the
gate cannot be skipped even in bypass-permissions mode.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Rewrite PROMPT.md with subagent orchestration pipeline

**Files:**
- Modify: `PROMPT.md` (complete rewrite)

- [ ] **Step 1: Write the new PROMPT.md**

Replace the entire contents of `PROMPT.md` with the subagent-driven orchestration prompt. The prompt has these sections:

**Preamble** — Identity, one-task-per-session rule, full system access principle.

**Phase 0: Orient** — Read AGENTS.md (not design docs — subagents read those). Run cargo build, cargo test, git log, git status. Note HEAD commit hash as `BASE_COMMIT`. Handle uncommitted work.

**Phase 1: Select Task** — Query Linear, pick highest-priority unblocked, move to In Progress, leave comment.

**Phase 2: Plan (subagent)** — Spawn planning subagent with prompt that:
- Reads the Linear issue description (passed in the prompt)
- Reads design docs (prd, system_design, ui_design)
- Reads relevant existing code
- Uses superpowers brainstorming and writing-plans skills to produce a structured spec
- Writes spec to `docs/specs/VEI-XX-<slug>.md`
- Spec must contain: context, implementation units, test strategy per unit, acceptance criteria, dependencies

Main agent reads the spec, commits: `plan(VEI-XX): design spec for <feature>`

**Phase 3: Write Failing Tests (subagent)** — Spawn test-writing subagent with prompt that:
- Reads the spec file at the path from Phase 2
- Reads existing codebase to understand structure
- Writes ALL failing tests for ALL implementation units
- Each unit's tests go in clearly separated modules
- Tests must compile but fail (RED)

Main agent runs `cargo test` to verify RED state. Commits: `test(VEI-XX): RED for <units>`

**Phase 4: Implement (one subagent per unit)** — For each unit from the spec:
- Spawn implementation subagent with: spec path, unit name, test file paths
- Subagent writes minimal code to make that unit's tests pass
- Main agent runs `cargo test` to verify
- If still failing after 2 retries with fresh subagents, leave comment on Linear and stop
- Main agent commits: `feat(VEI-XX): implement <unit>`

**Phase 5: Refactor (subagent)** — Spawn refactoring subagent with:
- List of changed files from `git diff --name-only $BASE_COMMIT`
- Instructions: improve names, reduce complexity, ensure single responsibility per file, split mixed concerns
- Main agent runs full quality gate, commits: `refactor(VEI-XX): <what improved>`

**Phase 6: Review (subagent loop, max 3 iterations)** — Each iteration spawns a NEW subagent that:
- Runs coderabbit review against BASE_COMMIT
- Evaluates and fixes legitimate feedback
- Main agent runs quality gate, commits fixes
- Loop until clean or max iterations reached

**Phase 7: Push** — `git push origin ralph-loop`. Hooks enforce quality gate.

**Phase 8: Update Linear and STOP** — Move to Done, leave summary comment. END RESPONSE.

**Guardrails** — Updated from v2:
- No placeholders/stubs
- No code outside workspace structure
- Search before creating files
- Focus on first compilation error
- Stop if going in circles
- Test behavior not implementation
- Single responsibility per file (not line count)
- Do NOT modify AGENTS.md
- Documentation near code
- Full system access — install dependencies as needed, document in commit messages
- Do NOT create new Linear issues

Full prompt content follows:

```markdown
# Veil — Ralph Loop Prompt

You are an autonomous development agent working on Veil, a cross-platform GPU-accelerated terminal workspace manager. You are running inside a ralph loop — each invocation gives you a fresh context window. All state between iterations persists ONLY on disk (files, git, SQLite, Linear).

**CRITICAL: Complete exactly ONE Linear task per session, then STOP. After Phase 8, end your response immediately. Do NOT loop back to Phase 0. Do NOT start a second task. The shell script will terminate this process and start a fresh one for the next task.**

**You have full system access** (bypass permissions mode). If a task requires installing tools (Zig, bindgen, etc.), cloning repositories, or setting up build dependencies, DO IT. Do not declare a task blocked because a dependency isn't pre-installed. Setting up the environment IS part of the work. Use `brew install`, direct downloads, or whatever is appropriate for macOS arm64. If you install something on the host system, document what you installed in the commit message.

## Phase 0: Orient

1. Read `AGENTS.md` for build commands, conventions, and project structure.
2. Run `cargo build 2>&1` — if no Cargo.toml, project scaffolding is the first task.
3. Run `cargo test 2>&1` �� note the current test status.
4. Run `git log --oneline -10` — understand what has been done.
5. Run `git status` — check for uncommitted work from a previous iteration.
   - If uncommitted work compiles and passes tests, commit it.
   - If it does NOT compile, try to fix it quickly. Otherwise `git stash` it.
6. **Record the current HEAD commit hash as `BASE_COMMIT`.** You will need this for coderabbit later.

## Phase 1: Select Task from Linear

1. List issues from team "Veil-term" in "Backlog" or "Todo" status.
2. Check `blockedBy` relationships — skip blocked issues.
3. Select the HIGHEST PRIORITY UNBLOCKED issue.
4. Move it to "In Progress".
5. Leave a comment: "Starting work on this issue."

If ALL issues are done or blocked, end your response.

## Phase 2: Plan (subagent)

Spawn a subagent with this prompt structure:

> You are a planning agent for the Veil project. Your job is to read the task requirements and produce a detailed implementation spec.
>
> **Task:** [paste the Linear issue title and description]
>
> **Instructions:**
> 1. Read the design docs: `docs/prd/prd.md`, `docs/system_design/system_design.md`, `docs/ui_design/ui_design.md`
> 2. Read `AGENTS.md` for project structure and conventions
> 3. Search the existing codebase for related code, types, and modules
> 4. Try to invoke the superpowers `brainstorming` and `writing-plans` skills to structure your work. If skills are not available, produce the spec directly.
> 5. Write the spec to `docs/specs/VEI-XX-<slug>.md`
>
> The spec MUST contain:
> - **Context**: What this task does and why
> - **Implementation units**: Independent units of work. Each unit maps to a set of related functions/types that can be tested and implemented together. Name each unit clearly.
> - **Test strategy per unit**: What tests each unit needs — happy path, error cases, edge cases
> - **Acceptance criteria**: What "done" looks like
> - **Dependencies**: Any tools, libraries, or crates that need to be installed or created

After the subagent completes, read the spec file it produced. Commit:
```
plan(VEI-XX): design spec for <feature>
```

## Phase 3: Write Failing Tests (subagent)

Spawn a subagent with this prompt structure:

> You are a test-writing agent for the Veil project. Write ALL failing tests for a task.
>
> **Spec:** Read the spec at `docs/specs/VEI-XX-<slug>.md`
> **Codebase:** Read `AGENTS.md` for structure. Search existing code for related types and modules.
>
> **Instructions:**
> 1. For EACH implementation unit in the spec, write tests that cover: happy path, error cases, edge cases
> 2. Organize tests by unit — use separate test modules or clearly labeled sections
> 3. Tests must COMPILE but FAIL (RED state). Use the right assertions, reference types/functions that don't exist yet
> 4. Run `cargo test` to confirm tests compile and fail for the right reasons (missing implementation, not typos)
> 5. Run `cargo fmt` to ensure formatting is clean
> 6. Test BEHAVIOR, not implementation. No mocking unless absolutely necessary.

After the subagent completes, run `cargo test` yourself to verify RED state. Commit:
```
test(VEI-XX): RED for <brief description of what units are tested>
```

## Phase 4: Implement (one subagent per unit)

For EACH implementation unit defined in the spec, spawn a separate subagent:

> You are an implementation agent for the Veil project. Make a specific set of failing tests pass.
>
> **Spec:** Read `docs/specs/VEI-XX-<slug>.md` — focus on unit: "[unit name]"
> **Tests:** Read the test files to understand what behavior is expected
> **Codebase:** Read `AGENTS.md`, then read existing code for context
>
> **Instructions:**
> 1. Read the failing tests for your unit
> 2. Write the MINIMAL code to make those tests pass
> 3. No placeholder implementations. No TODO stubs. Real code only.
> 4. Run `cargo test` to verify your unit's tests pass
> 5. Run `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings` to ensure quality
> 6. Do NOT modify tests — only write implementation code

After each subagent completes, run `cargo test` yourself to verify. If the unit's tests still fail, retry with a fresh subagent (max 2 retries). Commit after each unit:
```
feat(VEI-XX): implement <unit name>
```

## Phase 5: Refactor (subagent)

Spawn a subagent:

> You are a refactoring agent for the Veil project. Review and clean up code written for a task.
>
> **Changed files:** [list output of `git diff --name-only $BASE_COMMIT`]
> **Spec:** Read `docs/specs/VEI-XX-<slug>.md` for context
>
> **Instructions:**
> 1. Read every changed file
> 2. Refactor for clarity: better names, reduced complexity, extracted functions
> 3. Ensure each file has a single cohesive responsibility — split files that mix concerns into focused modules
> 4. Do NOT change behavior — all existing tests must still pass
> 5. Run `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test`

After the subagent completes, run the full quality gate yourself:
```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test && cargo build
```
Commit:
```
refactor(VEI-XX): <what was improved>
```

## Phase 6: Review (subagent loop)

Run up to 3 review iterations. Each iteration spawns a NEW subagent (fresh context):

> You are a code review agent for the Veil project. Run coderabbit and fix issues.
>
> **Base commit:** $BASE_COMMIT
> **Instructions:**
> 1. Run: `coderabbit review --plain --type committed --base-commit $BASE_COMMIT`
> 2. Read the feedback and evaluate each item:
>    - **Fix** issues about: logic errors, missing edge cases, unsafe misuse, error handling gaps, missing test coverage, API design problems, resource leaks
>    - **Ignore** feedback about: formatting (cargo fmt handles it), clippy lints (already gated), purely stylistic preferences
>    - When ignoring, briefly note WHY
> 3. Fix legitimate issues
> 4. Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
> 5. Report what you fixed and what you ignored with reasons

After each review subagent, commit any fixes:
```
fix(VEI-XX): address coderabbit feedback — <summary>
```

If the subagent reports no actionable feedback, the review loop is done. Move to Phase 7.

## Phase 7: Push

Run `git push origin ralph-loop`.

The pre-push hook will run the quality gate automatically. If it denies the push, fix the issue and retry.

## Phase 8: Update Linear and STOP

1. Move the Linear issue to "Done".
2. Leave a summary comment: what was implemented, what tests were added, what was refactored, any follow-up work identified.
3. Do NOT create new Linear issues. The backlog is curated by the human operator.

**END YOUR RESPONSE NOW. Do not continue. Do not start another task. The ralph loop shell script will start a new process with fresh context for the next task.**

---

## Guardrails

1. Do NOT implement placeholder, stub, or skeleton implementations. Every function must do real work. If a dependency doesn't exist yet, implement it or leave a comment on the Linear issue explaining what's missing and end your response.

2. Do NOT create files or code outside the crate workspace structure defined in AGENTS.md.

3. Before creating any new file, search the codebase to confirm it doesn't already exist.

4. If you encounter a compilation error that fills your context, focus on the FIRST error only. Fix it, recompile, repeat.

5. If you find yourself going in circles (breaking and fixing the same thing), STOP. Commit what works, leave a comment on the Linear issue, and end your response.

6. When writing tests, test BEHAVIOR not implementation. No mocking unless absolutely necessary.

7. Each file should express one cohesive unit. The signal to split is mixed responsibilities, not line count.

8. Do NOT modify AGENTS.md. It is a stable reference document.

9. Documentation belongs near the code — inline comments, doc comments. No standalone doc commits.

10. Do NOT create new Linear issues. The backlog is curated by the human operator.
```

- [ ] **Step 2: Verify the prompt is well-formed**

Read the file back and check: all phases numbered correctly, no TODO/TBD placeholders, subagent prompts are complete.

- [ ] **Step 3: Commit**

```bash
git add PROMPT.md
git commit -m "feat: rewrite ralph loop prompt for v3 subagent architecture

Main agent becomes lean orchestrator. All heavy work delegated to
focused subagents with fresh context windows:
- Plan subagent (with superpowers skills)
- Test-writing subagent (RED phase)
- Implementation subagents (one per unit, GREEN phase)
- Refactor subagent
- Review subagents (coderabbit loop)

Specs committed per task as auditable artifacts at docs/specs/.
One task per session enforced with explicit STOP instruction.
Full system access principle for dependency installation.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Update AGENTS.md for v3

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: Update AGENTS.md**

Changes needed:
1. Replace the hard "~300 lines" file size rule with the single-responsibility principle
2. Update the "Code Review Gate" section to note it's now subagent-driven
3. Add a "Specs" section referencing `docs/specs/`
4. Remove "Create issues: if you discover missing work, create a new issue in Linear" from Linear Integration section
5. Update TDD Commit Cadence to include `plan(VEI-XX):` prefix

- [ ] **Step 2: Verify changes are consistent with PROMPT.md**

Read both files and check for contradictions.

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md
git commit -m "feat: update AGENTS.md for v3 subagent architecture

Add plan commit prefix, specs directory reference, single-responsibility
file principle. Remove agent issue creation. Align with PROMPT.md v3.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 4: Update ralph.sh banner for v3

**Files:**
- Modify: `ralph.sh`

- [ ] **Step 1: Update the version banner**

Change line 59 from:
```bash
echo -e "${CYAN}║           Veil Ralph Loop v1.0               ║${NC}"
```
to:
```bash
echo -e "${CYAN}║           Veil Ralph Loop v3.0               ║${NC}"
```

And line 60 from:
```bash
echo -e "${CYAN}║   Autonomous Development with Superpowers    ║${NC}"
```
to:
```bash
echo -e "${CYAN}║   Subagent-Driven Autonomous Development     ║${NC}"
```

- [ ] **Step 2: Commit**

```bash
git add ralph.sh
git commit -m "chore: bump ralph loop banner to v3.0

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 5: Add docs/specs/ to .gitignore exclusions and verify clean state

**Files:**
- Modify: `.gitignore` (if needed)
- Verify: all files committed, nothing stale

- [ ] **Step 1: Verify docs/specs/ is NOT gitignored**

Run: `git check-ignore docs/specs/test.md; echo "exit: $?"`
Expected: exit 1 (not ignored). The specs directory should be tracked.

- [ ] **Step 2: Run git status to verify clean working tree**

Run: `git status`
Expected: `nothing to commit, working tree clean`

- [ ] **Step 3: Run git log to verify all commits are in order**

Run: `git log --oneline -10`
Expected: 4 new commits on top of `cb398fe` (coderabbit config):
1. Task 1: pre-push hook
2. Task 2: PROMPT.md rewrite
3. Task 3: AGENTS.md update
4. Task 4: ralph.sh banner

- [ ] **Step 4: Push all changes**

```bash
git push origin ralph-loop
```

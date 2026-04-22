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

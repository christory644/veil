#!/usr/bin/env bash
set -euo pipefail

# Veil Ralph Loop
# Usage: ./ralph.sh [max_iterations]
# Example: ./ralph.sh 20    # Run 20 iterations then stop
# Example: ./ralph.sh        # Run indefinitely (Ctrl+C to stop)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROMPT_FILE="${SCRIPT_DIR}/PROMPT.md"
LOG_DIR="${SCRIPT_DIR}/ralph-logs"
MAX_ITERATIONS="${1:-0}"  # 0 = infinite

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Ensure log directory exists
mkdir -p "${LOG_DIR}"

# Verify prompt file exists
if [[ ! -f "${PROMPT_FILE}" ]]; then
    echo -e "${RED}ERROR: PROMPT.md not found at ${PROMPT_FILE}${NC}"
    exit 1
fi

# Verify claude-personal is available
if ! command -v claude-personal &> /dev/null; then
    echo -e "${RED}ERROR: claude-personal command not found${NC}"
    exit 1
fi

iteration=0
start_time=$(date +%s)

echo -e "${CYAN}╔══════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║           Veil Ralph Loop v1.0               ║${NC}"
echo -e "${CYAN}║   Autonomous Development with Superpowers    ║${NC}"
echo -e "${CYAN}╠══════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║${NC} Working dir: ${BLUE}${SCRIPT_DIR}${NC}"
echo -e "${CYAN}║${NC} Prompt:      ${BLUE}${PROMPT_FILE}${NC}"
echo -e "${CYAN}║${NC} Logs:        ${BLUE}${LOG_DIR}${NC}"
if [[ "${MAX_ITERATIONS}" -gt 0 ]]; then
    echo -e "${CYAN}║${NC} Max iters:   ${YELLOW}${MAX_ITERATIONS}${NC}"
else
    echo -e "${CYAN}║${NC} Max iters:   ${YELLOW}infinite (Ctrl+C to stop)${NC}"
fi
echo -e "${CYAN}╚══════════════════════════════════════════════╝${NC}"
echo ""

# Trap Ctrl+C for clean exit
trap 'echo -e "\n${YELLOW}Ralph loop interrupted after ${iteration} iterations.${NC}"; exit 0' INT TERM

while true; do
    iteration=$((iteration + 1))

    # Check iteration limit
    if [[ "${MAX_ITERATIONS}" -gt 0 ]] && [[ "${iteration}" -gt "${MAX_ITERATIONS}" ]]; then
        echo -e "${GREEN}Reached max iterations (${MAX_ITERATIONS}). Stopping.${NC}"
        break
    fi

    # Timestamps
    iter_start=$(date +%s)
    timestamp=$(date '+%Y-%m-%d_%H-%M-%S')
    log_file="${LOG_DIR}/iteration_${iteration}_${timestamp}.log"

    # Elapsed time
    elapsed=$(( $(date +%s) - start_time ))
    hours=$((elapsed / 3600))
    minutes=$(( (elapsed % 3600) / 60 ))

    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}Iteration ${iteration}${NC} | ${BLUE}$(date '+%H:%M:%S')${NC} | Elapsed: ${hours}h${minutes}m | Log: ${log_file##*/}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    # Run claude-personal with the prompt, from the worktree directory
    # --dangerously-skip-permissions: required for autonomous operation
    # --print: non-interactive mode (reads from stdin, prints output)
    # --model opus: use Opus for best quality
    # --verbose: show tool calls for observability
    # Pipe both stdout and stderr to tee for live output + logging
    set +e
    (
        cd "${SCRIPT_DIR}"
        cat "${PROMPT_FILE}" | claude-personal \
            --print \
            --dangerously-skip-permissions \
            --model opus \
            --verbose \
            2>&1
    ) | tee "${log_file}"
    exit_code=${PIPESTATUS[0]}
    set -e

    # Iteration summary
    iter_elapsed=$(( $(date +%s) - iter_start ))
    iter_minutes=$((iter_elapsed / 60))
    iter_seconds=$((iter_elapsed % 60))

    echo ""
    if [[ "${exit_code}" -eq 0 ]]; then
        echo -e "${GREEN}Iteration ${iteration} completed in ${iter_minutes}m${iter_seconds}s (exit: ${exit_code})${NC}"
    else
        echo -e "${YELLOW}Iteration ${iteration} ended in ${iter_minutes}m${iter_seconds}s (exit: ${exit_code})${NC}"
    fi

    # Brief pause between iterations to avoid hammering the API
    echo -e "${BLUE}Next iteration in 5 seconds... (Ctrl+C to stop)${NC}"
    sleep 5
done

total_elapsed=$(( $(date +%s) - start_time ))
total_hours=$((total_elapsed / 3600))
total_minutes=$(( (total_elapsed % 3600) / 60 ))
echo -e "\n${GREEN}Ralph loop complete. ${iteration} iterations in ${total_hours}h${total_minutes}m.${NC}"
echo -e "Logs: ${BLUE}${LOG_DIR}/${NC}"

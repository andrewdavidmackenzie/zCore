#!/usr/bin/env bash
# Read the cores count from a board config file.
# Usage: source this script, then call get_target_cores <board-or-config>
# Accepts either a bare board name ("qemu-aarch64") or a full selection
# ("petal.qemu-aarch64"); the board is the part after the last dot.
# Returns the core count (defaults to 1 if not found).

get_target_cores() {
    local board="${1:?Usage: get_target_cores <board-or-config>}"
    board="${board##*.}"
    local config="configs/boards/${board}.toml"
    local cores=""
    if [ -f "$config" ]; then
        cores=$(grep '^cores' "$config" 2>/dev/null | awk '{print $NF}')
    fi
    # Default to 1 if config missing, key absent, or value empty
    echo "${cores:-1}"
}

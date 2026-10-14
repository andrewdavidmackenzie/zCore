#!/usr/bin/env bash
# Read the cores count from the target config file.
# Usage: source this script, then call get_target_cores <arch>
# Returns the core count (defaults to 1 if not found).

get_target_cores() {
    local arch="${1:?Usage: get_target_cores <arch>}"
    local config="targets/qemu-${arch}.toml"
    if [ -f "$config" ]; then
        grep '^cores' "$config" 2>/dev/null | awk '{print $NF}' || echo 1
    else
        echo 1
    fi
}

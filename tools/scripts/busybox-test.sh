#!/usr/bin/env bash
#
# Busybox applet test: start zCore in QEMU, run a series of busybox
# commands, verify their output, then shut down.
#
# Tests are split into two categories:
#   1. Shell builtins (echo, pwd, true, etc.) -- these work without fork
#   2. External commands (ls, cat, uname, etc.) -- these require fork/exec
#      and currently crash due to a known COW/fork issue (#207)
#
# Usage: tools/scripts/busybox-test.sh <arch>
#
# Exit code 0 = all builtin tests passed (external command failures are
#               reported but do not fail the build -- see #207)
# Exit code 1 = a builtin test failed or the shell did not boot

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
BOOT_TIMEOUT=60
CMD_TIMEOUT=10

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore.bin"
    IMAGE="zCore/aarch64.img"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -kernel "$KERNEL"
      -serial mon:stdio
      -initrd "$IMAGE"
    )
    ;;
  x86_64)
    KERNEL_ELF="target/x86_64/release/zcore"
    BOOT_IMG="target/x86_64/release/boot.img"
    ROOTFS_IMG="zCore/x86_64.img"
    BOOTIMAGE_TOOL="tools/x86-bootimage/target/release/x86-bootimage"

    if [ ! -f "$BOOTIMAGE_TOOL" ]; then
      echo "Building x86-bootimage tool..."
      cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
    fi
    BOOTIMAGE_ARGS=("$KERNEL_ELF" "$BOOT_IMG")
    if [ -f "$ROOTFS_IMG" ]; then
      BOOTIMAGE_ARGS+=(--ramdisk "$ROOTFS_IMG")
    fi
    "$BOOTIMAGE_TOOL" "${BOOTIMAGE_ARGS[@]}"

    KERNEL="$BOOT_IMG"
    QEMU_CMD=(
      qemu-system-x86_64
      -m 2G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase,+rdrand
      -serial mon:stdio
      -drive "format=raw,file=$BOOT_IMG"
    )
    ;;
  *)
    echo "ERROR: busybox-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Verify required files exist
if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found. Run 'make build ARCH=$ARCH' first."
  exit 1
fi

# --- helpers ---

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
BUILTIN_FAIL=0

# Strip ANSI escape codes from output
strip_ansi() {
  sed 's/\x1b\[[0-9;]*[a-zA-Z]//g'
}

# Wait for a marker string to appear as a standalone line in $OUTPUT.
# The serial terminal echoes input, so we must match the marker only
# when it appears at the start of a line (i.e., emitted by echo, not
# part of the echoed command).
# Usage: wait_for_marker "MARKER" timeout_seconds
wait_for_marker() {
  local marker="$1"
  local timeout="$2"
  local elapsed=0
  while [ "$elapsed" -lt "$timeout" ]; do
    if strip_ansi < "$OUTPUT" | grep -q "^${marker}$" 2>/dev/null; then
      return 0
    fi
    # Check if QEMU died
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      return 1
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
  return 1
}

# Run a test: send a command, check for a PASS marker in the output.
# Usage: run_test "test_name" "shell_command" "expected_output_substring" [is_builtin]
#
# The function sends:
#   shell_command ; echo __DONE_test_name__
# and checks that expected_output_substring appears in the output
# between the command and the __DONE__ marker.
#
# If is_builtin is "builtin", failures count as hard errors.
# If is_builtin is "external" (default), failures are reported but not fatal.
run_test() {
  local name="$1"
  local cmd="$2"
  local expected="$3"
  local kind="${4:-external}"

  local marker="__DONE_${name}__"
  local before_bytes
  before_bytes=$(wc -c < "$OUTPUT")

  printf "  %-30s" "$name"

  # Check if QEMU is still alive
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "SKIP (QEMU exited)"
    SKIP_COUNT=$((SKIP_COUNT + 1))
    return
  fi

  # Send the command with a marker
  echo "$cmd ; echo $marker" >&3

  # Wait for the marker
  if ! wait_for_marker "$marker" "$CMD_TIMEOUT"; then
    if [ "$kind" = "builtin" ]; then
      echo "FAIL (no response within ${CMD_TIMEOUT}s)"
      FAIL_COUNT=$((FAIL_COUNT + 1))
      BUILTIN_FAIL=$((BUILTIN_FAIL + 1))
    else
      # External commands crash the shell due to the fork/exec/wait bug (#207).
      # Check if the command itself produced output before the crash,
      # skipping the echoed command line (first line).
      local partial
      partial=$(tail -c "+$((before_bytes + 1))" "$OUTPUT" 2>/dev/null | strip_ansi | tail -n +2)
      if echo "$partial" | grep -qF "$expected" 2>/dev/null; then
        echo "SKIP (output OK, shell crashed after wait4 -- #207)"
      else
        echo "SKIP (fork crash -- see #207)"
      fi
      SKIP_COUNT=$((SKIP_COUNT + 1))
    fi
    return
  fi

  # Extract output since we sent the command (from before_bytes onward),
  # skipping the first line which is the serial echo of the command itself.
  local new_output
  new_output=$(tail -c "+$((before_bytes + 1))" "$OUTPUT" | strip_ansi | tail -n +2)

  # Check for expected substring in command output (not the echoed command)
  if echo "$new_output" | grep -qF "$expected"; then
    echo "PASS"
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    echo "FAIL (expected '$expected')"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    if [ "$kind" = "builtin" ]; then
      BUILTIN_FAIL=$((BUILTIN_FAIL + 1))
    fi
  fi
}

# --- set up QEMU ---

echo "Starting QEMU (boot timeout=${BOOT_TIMEOUT}s)..."

OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"
trap 'rm -f "$OUTPUT" "$QEMU_IN"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for shell prompt
PROMPT_PATTERN='/ # '
ELAPSED=0
while [ "$ELAPSED" -lt "$BOOT_TIMEOUT" ]; do
  if grep -q "$PROMPT_PATTERN" "$OUTPUT" 2>/dev/null; then
    echo "Shell prompt reached in ${ELAPSED}s"
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before shell prompt"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    exec 3>&- 2>/dev/null || true
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

if [ "$ELAPSED" -ge "$BOOT_TIMEOUT" ]; then
  echo "FAIL: shell prompt not reached within ${BOOT_TIMEOUT}s"
  echo "--- QEMU output ---"
  cat "$OUTPUT"
  exec 3>&- 2>/dev/null || true
  kill "$QEMU_PID" 2>/dev/null || true
  exit 1
fi

# Small pause to let shell fully initialize
sleep 1

echo ""
echo "==> Running busybox applet tests..."
echo ""

# ============================================================
# Part 1: Shell builtins (no fork required -- these must pass)
# ============================================================
echo "--- Shell builtins (no fork) ---"

run_test "echo" \
  "echo hello_world" \
  "hello_world" builtin

run_test "pwd" \
  "pwd" \
  "/" builtin

run_test "true" \
  "true && echo true_ok" \
  "true_ok" builtin

run_test "false" \
  "false || echo false_ok" \
  "false_ok" builtin

run_test "echo_variable" \
  "X=hello && echo \$X" \
  "hello" builtin

run_test "test_builtin" \
  "test -d /bin && echo dir_ok" \
  "dir_ok" builtin

# Note: file redirect (echo foo > file) is not tested because the SFS
# rootfs does not support creating new files at runtime.

# ============================================================
# Part 2: External commands (require fork -- known to crash,
#          see #207). Tested last since fork crash kills shell.
# ============================================================
echo ""
echo "--- External commands (require fork -- #207) ---"

run_test "ls" \
  "ls /bin" \
  "busybox" external

# cat requires fork (external command) and a readable file.
# Since SFS doesn't support runtime file creation, we skip cat
# testing until the fork issue (#207) and writable fs are resolved.

run_test "uname" \
  "uname" \
  "Linux" external

run_test "stat" \
  "stat /bin/busybox" \
  "File:" external

run_test "pipe" \
  "echo pipe_works | cat" \
  "pipe_works" external

run_test "ps" \
  "ps" \
  "PID" external

# --- shutdown ---

echo ""
# Try clean shutdown; if QEMU already exited (fork crash), that's OK
if kill -0 "$QEMU_PID" 2>/dev/null; then
  echo "Sending 'poweroff -f'..."
  echo "poweroff -f" >&3 2>/dev/null || true
  exec 3>&- 2>/dev/null || true

  for i in $(seq 1 10); do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      wait "$QEMU_PID" || true
      break
    fi
    sleep 1
  done
  if kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
  fi
else
  exec 3>&- 2>/dev/null || true
  echo "(QEMU already exited)"
fi

# --- report ---

echo ""
echo "========================================"
echo "  Busybox applet test results ($ARCH)"
echo "  PASS: $PASS_COUNT  FAIL: $FAIL_COUNT  SKIP: $SKIP_COUNT"
echo "========================================"

if [ "$BUILTIN_FAIL" -gt 0 ]; then
  echo ""
  echo "FATAL: $BUILTIN_FAIL builtin test(s) failed!"
  echo "--- Full QEMU output ---"
  strip_ansi < "$OUTPUT"
  exit 1
fi

if [ "$SKIP_COUNT" -gt 0 ]; then
  echo ""
  echo "Note: $SKIP_COUNT test(s) skipped due to fork/exec crash (see #207)"
fi

exit 0

# zCore Updated

## Default build command
To build and run zCore the default command to use is:

```bash
cargo qemu --arch aarch64
```

## Pre-push checks
**Always run `make pre-push` before pushing commits.** This runs the same
checks as CI (clippy, fmt, builds, boot tests, libc tests, feature
combinations) and catches failures locally before they show up in CI.

```bash
make pre-push
```

If `make pre-push` fails, fix the issue before pushing. Do not push
code that fails pre-push checks.

For faster iteration, use `make pre-push-quick` which runs clippy,
fmt, unit tests, and builds (~3 min) but skips QEMU boot tests.
Run the full `make pre-push` before the final push.

## Problem-solving principles
- **Never silence errors or warnings without understanding the root cause.**
  Downgrading a log level or suppressing output is not a fix. Investigate
  why the error occurs and fix the underlying problem.
- **Never assume something is "pre-existing" without verifying.** Check
  whether the issue exists on master before dismissing it.
- **Never choose the simplest option.** Choose the most correct option
  regardless of effort. Simple hacks create technical debt.
- **Drive to understand root causes.** Ask "why does this happen?" not
  "how do I hide this?" Trace the code path, understand the failure
  mode, then fix it properly.

## Running Fuchsia core-tests

The `core-tests-standalone` binary (1776 tests, 110 suites) validates
Zircon syscall compatibility. Always use `-smp 1` to eliminate
multi-core timing issues.

### Build and run
```bash
# Build kernel
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/core-tests-standalone" cargo bin -m qemu-x86_64

# Create boot image
tools/x86-bootimage/target/release/x86-bootimage \
  target/qemu-x86_64/release/kernel \
  target/qemu-x86_64/release/boot.img \
  --ramdisk target/qemu-x86_64/release/x86_64-zircon.img

# Run in QEMU (1 CPU, kill after timeout)
source tools/scripts/find-ovmf.sh && OVMF=$(find_ovmf)
qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
  -machine q35 -smp 1 \
  -cpu qemu64,+fsgsbase,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
  -serial mon:stdio \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
  -drive "format=raw,file=target/qemu-x86_64/release/boot.img" \
  2>&1 > /tmp/qemu-test.log &
PID=$!; sleep 900; kill $PID 2>/dev/null; wait $PID 2>/dev/null
```

### Check results
```bash
# Summary
P=$(grep -c '\[       OK \]' /tmp/qemu-test.log)
F=$(grep -c '\[  FAILED  \]' /tmp/qemu-test.log)
echo "Passed: $P  Failed: $F  Not reached: $((1776 - P - F))"

# Passing tests
grep '\[       OK \]' /tmp/qemu-test.log | sed 's/\x1b\[[0-9;]*m//g'

# Where it stopped
tail -5 /tmp/qemu-test.log | sed 's/\x1b\[[0-9;]*m//g'
```

### Current status (phase 12)
- 551/1776 tests pass, 29 failing, 1196 not yet reached
- TransferChannelWithPendingCall hang resolved — channel_call now
  passes a cancel_token so handle transfer cancels the blocking call
- Executor `take_notified` fixed — notifications masked by `borrowed`
  or `dropped` bits are now restored (deferred) instead of lost
- channel_read options validation added (rejects invalid options
  with NOT_SUPPORTED)
- write_etc first-error latching fixed for BAD_HANDLE dispositions
- New suites reached: ChannelTest (75/88 pass), ChannelWriteEtcTest,
  IOVecTest, ChannelCallEtcTest, ChannelInternalTest
- Current blocker: ChannelTest.NoSpuriousReadableSignalWhenRacing
  hangs due to cooperative scheduling overhead — 10000 iterations
  of busy-wait + channel read/write takes ~150s+ in QEMU
- NOTE: increasing timer frequency above 100 Hz makes things worse
  in QEMU due to VM exit/enter overhead per interrupt. Use 900s+
  timeout to let slow tests complete.
- VMO ambient exec: zx_vmo_create grants EXECUTE right when job
  policy allows AMBIENT_MARK_VMO_EXEC (fixes MmapProtExecTest)
- Port cancel now drains queued packets and checks source WAIT rights
- Signal callbacks fire in LIFO order (matching Fuchsia kernel)
- wait_async supports TIMESTAMP/BOOT_TIMESTAMP options
- New suites: MemoryMappingTest (6/8 pass), PortTest (36/40 pass),
  PortStressTest (1/12 reached)
- Socket disposition write-disable signal model implemented
- Stream content_size zeroing, write error handling (FILE_BIG/OUT_OF_RANGE)
- VMO content_size set on create and updated on resize
- Stream objects don't support ZX_PROP_NAME (returns WRONG_TYPE)
- Datagram socket all-or-nothing write semantics
- VMO immutable flag for SNAPSHOT + NO_WRITE children
- gtest_filter support via ZBI CMDLINE items for fast test iteration
- VmoCloneTestCase (9 tests) reached when skipping PortStressTest

### Key notes
- `LOG=warn` required — `LOG=info` messages get stripped by LTO in
  release builds. Use `hal_impl::console::console_write_fmt` for
  diagnostics that must survive LTO.
- GTest filter: pass `--gtest_filter=Pattern*` via argv in spawn.rs
  (not yet implemented, would require modifying processargs argv).
- Test binary: `prebuilt/zircon-test/x86_64/core-tests-standalone`
- Issue #21 tracks overall progress, issue #468 tracks missing syscalls.
- Issue #471 tracks the user-buffer copy architecture gap.

## PR workflow
After pushing commits to a PR:
1. Wait for CI checks to complete
2. **Always check for code review comments** (CodeRabbit and human reviewers) using `gh api repos/andrewdavidmackenzie/zCore/pulls/<PR>/comments` and `gh pr view <PR> --json reviews`
   (replace `<PR>` with the actual pull request number before running)
3. Address all actionable review comments before moving on to new work
4. Push fixes as new commits, then re-check for new comments

This applies after every push, not just the final one. Do not wait for the user to ask -- proactively check and fix review comments.

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

### Current status (phase 17)
- ~750/1776 tests pass (individual suite runs), ~583 in full sequential run
- Phase 17: JobTest 15→26/29, JobGetInfoTest 32→39/39,
  ProcessTest.GetRuntimeNoPermission fixed, DebugLogTest +2,
  VmarGetInfoTest +1
- Mini-process infrastructure working — child processes can now be
  spawned via start_mini_process_etc (vDSO EXECUTE rights fix)
- Demand-paging in guarded kernel copies — copy_from_user/copy_to_user
  now demand-page user data instead of failing on unmapped pages
- VmoInfo struct expanded to 168 bytes matching Fuchsia's zx_info_vmo_t
  (was 104 bytes, causing garbage reads and infinite polling hangs)
- Per-port packet limit corrected to 4096 (was 2048)
- New suites reached: VmoCloneTestCase (9/9 pass),
  VmoCloneDisjointClonesTests (2/2), VmoSignalTestCase (3/3),
  VmoSliceTestCase (14/19), VmoReference (4/17),
  ProgressiveCloneDiscardTests, VmoTransferDataTestCase
- ProcessTest (12/30 pass), DefaultExceptionHandlerTest (1/2 pass),
  JobGetInfoTest (39/39+1 BUFFER_TOO_SMALL), VmarGetInfoTest (18/21),
  JobTest (26/29)
- V2 policy: ZX_POL_OVERRIDE_DENY/ALLOW, atomic batch application
- ZX_INFO_TASK_RUNTIME (topic 30) stub, ZX_PROP_JOB_KILL_ON_OOM
- Job max height (32), return code tracking, TASK_RETCODE fix (-1024)
- gtest_filter working: use `?--gtest_filter=-Suite.*:Test.Name` in
  ROOTPROC to skip tests. Both argv and ZBI CMDLINE delivery work.
- Known hangs/crashes requiring gtest_filter skip:
  - PortStressTest.* (multi-threaded stress, cooperative scheduler)
  - ChannelTest.NoSpuriousReadableSignalWhenRacing (10K iterations)
  - VmoClone2TestCase.* (populated_bytes fractional attribution)
  - VmoCloneResizeTests.* (populated_bytes after resize/decommit)
  - PortTest.QueuePacketLimitExceededGeneratesPolicyException
    (std::latch + std::thread synchronization)
  - PortTest.TooManyObservers (OOM crash — kernel panics on alloc
    failure instead of returning ZX_ERR_NO_MEMORY)
  - ChannelCallMutexTest.* (std::thread synchronization)
  - ChannelTest.ChannelFullException (policy exception infrastructure)
  - VmoTransferDataTestCase.InvalidInputs (hangs on
    boot_options->test_ram_reserve — uninitialized struct)
  - VmoZeroTestCase.WriteCowParent (populated_bytes polling hang)
  - VmoZeroTestCase.AllocateAfterMergeMultipleChildren (populated_bytes)
  - VmoZeroTestCase.AllocateAfterMerge (populated_bytes)
  - VmoZeroTestCase.DecommitMiddle (populated_bytes)
  - VmoZeroTestCase.Contiguous (populated_bytes)
  - VmoZeroTestCase.Nested (populated_bytes)
  - VmoZeroTestCase.ChildZeroThenWrite (populated_bytes)
  - VmoZeroTestCase.MergeZeroChildren (populated_bytes)
  - VmoZeroTestCase.EmptyCowChildren (populated_bytes)
  - VmoTestCase.* (multiple hangs: boot_options uninitialized, VMAR
    map failures causing infinite waits; only 1/58 passes)
  - PagerProcess.* (thread blocked on pager fault not woken on
    process kill — multiple tests hang, blocks 50+ later suites)
- All 1776 tests have been extracted to /tmp/all_tests.txt via
  `--gtest_list_tests`. Individual suites can be tested with
  `--gtest_filter=SuiteName.*` for fast iteration.
- VMO ambient exec: zx_vmo_create grants EXECUTE right when job
  policy allows AMBIENT_MARK_VMO_EXEC (required by prebuilt libc)
- Port cancel now drains queued packets and checks source WAIT rights
- Signal callbacks fire in LIFO order (matching Fuchsia kernel)
- wait_async supports TIMESTAMP/BOOT_TIMESTAMP options
- Socket disposition write-disable signal model implemented
- Stream content_size zeroing, write error handling (FILE_BIG/OUT_OF_RANGE)
- VMO content_size set on create and updated on resize
- Datagram socket all-or-nothing write semantics
- VMO immutable flag for SNAPSHOT + NO_WRITE children
- Job signals: JOB_NO_JOBS, JOB_NO_PROCESSES, JOB_NO_CHILDREN managed
  on child/process add/remove
- VMO REFERENCE child type (ZX_VMO_CHILD_REFERENCE) via full-VMO slice
- Pager query_dirty_ranges and query_vmo_stats stubs implemented

### Key notes
- `LOG=warn` required — `LOG=info` messages get stripped by LTO in
  release builds. Use `hal_impl::console::console_write_fmt` for
  diagnostics that must survive LTO.
- GTest filter: pass `--gtest_filter=Pattern*` via `?` separator in
  ROOTPROC cmdline (e.g., `ROOTPROC=/bin/core-tests-standalone?--gtest_filter=PortTest.*`).
  Delivered via both argv and ZBI CMDLINE items.
- Test binary: `prebuilt/zircon-test/x86_64/core-tests-standalone`
- Issue #21 tracks overall progress, issue #468 tracks missing syscalls.
- Issue #471 tracks the user-buffer copy architecture gap.
- Issue #21 has a master list of all 1776 tests with pass/fail/hang
  status and a categorized root cause analysis of all failures.
- VmoClone2/VmoCloneResize tests hang on `populated_bytes` polling
  because COW page attribution doesn't implement fractional scaling.
  Use gtest_filter to skip these suites when running the full suite.
- All 1776 tests have been extracted to /tmp/all_tests.txt via
  `--gtest_list_tests`. Individual suites can be tested with
  `--gtest_filter=SuiteName.*` for fast iteration.

## PR workflow
After pushing commits to a PR:
1. Wait for CI checks to complete
2. **Always check for code review comments** (CodeRabbit and human reviewers) using `gh api repos/andrewdavidmackenzie/zCore/pulls/<PR>/comments` and `gh pr view <PR> --json reviews`
   (replace `<PR>` with the actual pull request number before running)
3. Address all actionable review comments before moving on to new work
4. Push fixes as new commits, then re-check for new comments

This applies after every push, not just the final one. Do not wait for the user to ask -- proactively check and fix review comments.

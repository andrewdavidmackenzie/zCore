# Handoff: Fix 4 Skipped Core-Tests

## Context
PR #813 fixed 8 previously-skipped tests and added the FSGSBASE MSR
fallback in the vendored trapframe. The full suite (1778 tests) now
runs with 4 skipped, 1770 passing, 0 failing — stable on both
ThinkPad (5/5 runs) and CI.

## The 4 Skipped Tests

### 1. `Pager.FailEarlyWake`
- **Symptom**: Hangs 4/5 runs in the full suite. Passes alone.
- **GDB data**: All CPUs in `wait_for_interrupt`. No allocator
  corruption (self-loop detector silent).
- **Scan instrumentation**: `scan_notifications` keeps running but
  `found_total` stops growing — waker notification bits are never set.
- **Test**: Creates a 3-page pager VMO, supplies page 0, then
  fails page 1 via `FailPages`. The thread should wake with IO error.
- **Root cause hypothesis**: The `FailPages` path sets per-waiter
  errors but the waker callback that should fire `wake_by_ref()` on
  the waiting thread is lost. The test passes alone because timing
  differs — the thread is already blocked when the fail arrives.
  In the suite, prior test cleanup may leave the port or signal
  system in a state where the callback doesn't fire.

### 2. `PagerWriteback.NoDirtyRequestsForClones`
- **Symptom**: Hangs consistently (both with and without `+fsgsbase`).
- **Test**: Creates a TRAP_DIRTY pager VMO, clones it, writes to
  clone (should NOT trigger dirty requests), then writes to parent
  (SHOULD trigger dirty). Thread t2 waits for dirty acknowledgment.
- **Root cause hypothesis**: The TRAP_DIRTY dirty-request path has
  a notification loss — the pager's port never receives the
  `ZX_PAGER_VMO_DIRTY` packet, or the thread waiting for dirty
  approval is never woken after `DirtyPages` is called.

### 3. `VmoTestCase.ParallelWriteAndDecommit`
- **Symptom**: Intermittent hang. Sometimes passes (3s), sometimes
  hangs forever.
- **Test**: 4 threads (2 writers, 2 decommitters) racing on a
  1-page VMO mapped R/W. Writers do `memset(dst, 0x1, 128)` 100K
  times. Decommitters call `ZX_VMO_OP_DECOMMIT` 100K times.
- **Root cause hypothesis**: Page fault on decommitted memory
  generates a wakeup that's lost between CPUs. Or the COW/decommit
  path has a signal notification bug under concurrent access.

### 4. `Pager.ResizeCommitTest`
- **Symptom**: Fails (not hangs) — `t.WaitForBlocked()` returns
  false. The test thread isn't in the blocked state when expected.
- **Test**: Commits 3 pages of a resizable pager VMO, supplies 1,
  resizes to 1 page (removing 2), then supplies page 0. Checks
  accounting after resize.
- **Root cause hypothesis**: Timing issue — `WaitForBlocked` polls
  too early before the thread has entered `BlockedPager` state.
  May need a retry loop or the kernel should set the blocked state
  earlier in the pager wait path.

## Key Infrastructure for Debugging

### Vendored crates
- `vendor/trapframe/` — FSGSBASE MSR fallback in `syscall.S`
- `vendor/buddy_system_allocator/` — self-loop panic detector in
  `linked_list.rs` `IterMut::next()`

### Instrumentation (can be removed once bugs are fixed)
- `executor/src/task_collection.rs`: `scan_notifications` logs
  scan count, found count, task count, and waker page notified
  bits every 500K scans (only when `task_num > 0`)
- `executor/src/waker_page.rs`: `peek_notified()` method for
  read-without-clear diagnostics

### GDB debugging recipe
```bash
# Start QEMU with GDB port, serial to file
qemu-system-x86_64 ... -serial file:/tmp/qemu-core-tests.log -gdb tcp::1234 &

# Wait for hang, then attach
GDB=/opt/RustRover-2026.2.2/bin/gdb/linux/x64/bin/gdb
$GDB target/qemu-x86_64/release/kernel -batch \
  -ex 'set pagination off' \
  -ex 'target remote localhost:1234' \
  -ex 'thread apply all bt 15'
```

### Test filtering
```bash
# Run specific test
make core-tests CORE_TESTS_FILTER='Pager.FailEarlyWake' CORE_TESTS_TIMEOUT=60

# Run full suite with skip list
make core-tests CORE_TESTS_TIMEOUT=1800
```

## Fuchsia Source Locations
- Pager tests: `zircon/system/utest/core/pager/pager.cc`
- PagerWriteback tests: `zircon/system/utest/core/pager-writeback/pager-writeback.cc`
- VMO tests: `zircon/system/utest/core/vmo/vmo.cc`
- Fuchsia pager signal delivery: `zircon/kernel/object/pager_dispatcher.cc`
- Fuchsia VMO pager: `zircon/kernel/vm/vm_cow_pages.cc`

## Common Pattern Across All 4
A Zircon kernel event (page supply, page fail, dirty ack, signal)
should trigger a callback that calls `wake_by_ref()` on a waiting
task's waker. The waker sets a notification bit on the executor's
waker page and sends an IPI. The notification is never set —
confirmed by scan instrumentation showing `found_total` frozen
and `notified_bits=0x0` during hangs.

The bug is in the kernel's signal/callback delivery, NOT in the
executor's notification scanning (which continues running).

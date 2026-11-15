# Handoff: Mutex Refactor and Deadlock Fixes

## PR #814: Current State

Branch: `fix-deadlocks-and-mutex-refactor`

### What works
- **L1↔L4 deadlock fix**: `handle_page_fault` drops page_table lock
  before dirty tracking. Fixes `VmoTestCase.ParallelWriteAndDecommit`
  and `Pager.FailEarlyWake`.
- **Lockdep checker**: Runtime lock ordering verification behind
  `lockdep` feature flag.
- **KernelMutex**: Adaptive spin+yield mutex in `lock` crate with
  registered yield callback.
- **InterruptSpinLock**: Renamed from TicketMutex, kept for IRQ-context locks.
- **Test tooling**: Improved `core-test-run` and `core-tests` targets.
- **ThinkPad x86_64**: 1399/1776 pass, 0 fail (up from ~1306 baseline).

### What's broken
- **aarch64 CI failures**: Zircon petal tests (vmo-test, process-mem-test,
  exception-test) hang on aarch64 QEMU. Caused by KernelMutex not
  disabling interrupts — aarch64 code paths have races when interrupts
  can fire during kernel object lock critical sections.
- The KernelMutex conversion may have been premature. The interrupt-
  disabling was masking real concurrency issues.

### Recommended path forward
1. **Split the PR**: Extract the deadlock fix (handle_page_fault L4→L1)
   and lockdep checker into a separate, smaller PR. These are the high-
   value fixes that don't change mutex semantics.
2. **Revert the KernelMutex conversion** for now — keep TicketMutex
   (interrupt-disabling) for all locks. The deadlock fix + lockdep
   ordering is sufficient to prevent the ABBA deadlock class.
3. **Investigate aarch64 races**: Before converting to non-interrupt-
   disabling mutexes, identify which code paths rely on interrupt-
   disable for correctness (not just for the lock itself).
4. **Register yield function**: The KernelMutex yield callback
   (`set_yield_fn`) was never called in kernel init. If/when we
   re-enable KernelMutex, this needs to be registered.

### Skip list (4 tests)
1. `PagerWriteback.NoDirtyRequestsForClones` — sys_vmo_write on clone
   tree needs async pager demand-paging
2. `Pager.ResizeCommitTest` — overlap waiter gets wrong error when
   outstanding request completes during resize
3. `Threads.DebugRegistersWatchpointSyscallPanicRepro` — hangs, blocks
   remaining tests
4. `Pager.FailEarlyWake` — passes alone and in Pager.* suite, hangs
   in full suite (contamination from earlier test)

### ThinkPad access
```bash
ssh thinkpad 'cd ~/RustroverProjects/zCore && make core-tests CORE_TESTS_TIMEOUT=900'
ssh thinkpad 'cd ~/RustroverProjects/zCore && make core-test-run T="TestName" CORE_TEST_TIMEOUT=60'
rsync -az zCore/path/to/file.rs thinkpad:~/RustroverProjects/zCore/zCore/path/to/file.rs
```

### Key files changed
- `zCore/lock/src/mutex.rs` — KernelMutex implementation
- `zCore/lock/src/lockdep.rs` — Lock ordering checker
- `zCore/lock/src/ticket.rs` — InterruptSpinLock (renamed TicketMutex)
- `zCore/zircon-object/src/vm/vmar.rs:2595` — Deadlock fix (L4 drop)
- `zCore/zircon-object/src/mutex.rs` — Re-exports from lock crate

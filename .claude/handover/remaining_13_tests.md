# Remaining 13 Hanging Tests — Handover Document

## Current State (after PR #800 + PR #802)

### Skip list: 372 → 13 (359 removed, 96.5%)

### Branch: `preemptive_thread_scheduling_801` (PR #802)

### Test counts
- Rust core_tests: 3088 pass, 0 fail, 6 ignored
- C++ skip list: 13
- aarch64 + x86_64 boot tests: all pass

## Remaining 13 tests (all genuine hangs)

| Test | Category | Hang point |
|------|----------|-----------|
| PagerWriteback.LargeWriteBatched | TestThread vmo.write | Main thread stuck before port_wait |
| PagerWriteback.ResizeWritebackWithMappingTrapDirty | TestThread mapping write | Same pattern |
| PagerWriteback.SuspendDirtyTestVmar | Suspend during dirty fault | Same pattern |
| Pager.OverlapCommitSupplyTest | Multi-thread overlap commits | WaitForBlocked fails |
| Pager.DetachNonMappingAccess | vmo_read straddling 2 pages + detach | Main thread never reaches port_wait |
| Pager.SuspendReadTestVmar | Suspend during VMAR pager read | Same pattern |
| Threads.DebugRegistersDr6CorrectAfterBreakpointRemoval | Hardware breakpoint exception | Needs INT1 delivery |
| Threads.DebugRegistersDr6ResetOnDebugException | Hardware debug exception | Same |
| VmoClone2TestCase.DropChildrenInParallel | Concurrent VMO drop race | Multi-thread race |
| VmoClone2TestCase.ParentVmarWrite | 16K page issue on macOS | aarch64-macOS specific |
| PortStressTest.ChannelCallWaitQueuePanicRepro | channel_call infrastructure | channel_call not implemented |
| Bti.DecommitRace | BTI decommit race condition | Multi-thread BTI race |
| StreamTestCase.PartialVmoDirty | Stream + TRAP_DIRTY pager | Pager + stream interaction |

## Key findings from investigation

### Cross-CPU wakeup WORKS
Verified with logging: port signal → waker → IPI → executor wake
chain functions correctly. The remaining hangs are NOT caused by
the cooperative executor failing to schedule threads.

### The common hang pattern
Most remaining tests follow this pattern:
1. C++ test creates a TestThread via `thrd_create` (std::thread)
2. TestThread starts and does an operation (vmo.write, COMMIT, etc.)
3. Main thread should call `port_wait` to handle pager requests
4. Main thread NEVER REACHES `port_wait`

The main thread is stuck in C++ userspace code between `t.Start()`
return and the next syscall (`WaitForPageRead`). Timer preemption
at user-kernel boundaries works, so both threads execute. But the
main thread doesn't make progress through its C++ code.

### What was ruled out
- **Futex race**: Fixed (waker stored before waiter registration)
  but NOT the root cause of these hangs
- **Cooperative executor**: Timer interrupts DO preempt user threads
  at kernel boundaries. Both CPUs (0 and 7) run concurrently.
- **Port signal wakeup**: IPI mechanism verified working
- **COW clone resolution**: Fixed with pager_root + offset translation

### What needs investigation
The main thread is executing C++ userspace code but not reaching
its next syscall. Possible causes:
1. The C++ `std::thread` constructor or `TestThread::Start()` blocks
   on something other than a futex (e.g., `pthread_barrier_wait`)
2. The main thread's C++ code between Start() and WaitForPageRead
   calls another syscall that hangs (not futex_wait, not port_wait)
3. Memory ordering issue: the TestThread's writes aren't visible
   to the main thread's reads (unlikely with atomic operations)

### Diagnostic approach
Add warn-level logging to ALL syscalls made by the test thread
name to trace the exact sequence. The `handler_user_trap` function
could log syscall numbers for specific thread names.

## Kernel fixes made in this session

### PR #800 (merged)
1. PAGE_SIZE consolidation (7→1 definitions)
2. system_get_page_size syscall for petal/userstart
3. OP_ZERO dirty tracking (3 fixes)
4. COW clone zero marker without COW fork
5. Pager rights checks (ATTACH_VMO, MANAGE_VMO)
6. set_stream_size modified flag
7. IPI wakeup on cross-CPU spawn
8. BAD_SYSCALL vDSO policy check (+ aarch64 ELR fix)
9. rflags RF bit sanitization
10. Detach unmap + BAD_STATE fault
11. COW clone pager_root propagation + offset translation
12. COMMIT on TRAP_DIRTY skips dirty requests
13. BTI pin 3-phase dirty request flow
14. OP_ZERO preflight for pinned pages

### PR #802 (open)
15. Page fault handler clone offset translation
16. COW write sibling data copy (pager tree)
17. in_pager_tree flag on VMObjectPagedInner
18. Commit/Prefetch root page offset translation
19. Futex waker race fix

## Testing commands

```bash
# Run all Rust tests
cargo test -p zircon-tests --lib -- --test-threads=1

# Run specific test on QEMU
make core-tests CORE_TESTS_FILTER='TestName' CORE_TESTS_TIMEOUT=30

# Run aarch64 boot test
bash tools/scripts/zircon-boot-test.sh aarch64

# Run x86_64 boot test
bash tools/scripts/zircon-boot-test.sh x86_64

# Full pre-push
make pre-push
```

## Key files

- Skip list: `tests/core-tests-skip.txt`
- Futex implementation: `zCore/zircon-object/src/signal/futex.rs`
- Page fault handler: `zCore/zircon-object/src/vm/vmar.rs` (handle_page_fault)
- COW clone tree: `zCore/zircon-object/src/vm/vmo/paged.rs` (commit_page_internal)
- Pager VMO: `zCore/zircon-object/src/vm/vmo/mod.rs`
- Syscall dispatch: `zCore/zircon-loader/src/zircon.rs` (handler_user_trap)
- VMO read pager path: `zCore/zircon-syscall/src/vmo.rs` (sys_vmo_read)
- BTI pin: `zCore/zircon-syscall/src/ddk.rs` (sys_bti_pin)
- Commit/Prefetch: `zCore/zircon-syscall/src/vmo.rs` (sys_vmo_op_range_async)

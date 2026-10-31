# Session Progress — Handover Document

## Current State

### Branch: `preemptive_thread_scheduling_801` (PR #802)
### Latest Commit: `81dd2577` (fix: CI failures — fmt, Rust test assertions)

### Test Results
- **Rust zircon-tests**: 3096 pass, 0 fail, 1 ignored
- **C++ core-tests (x86_64 QEMU, ThinkPad)**: 1721 pass, 21 fail, 28 skip
- **CI**: Clippy fmt + Unit Test (ubuntu) failures fixed in latest commit, awaiting re-run

### Session Delta
- **Started**: 1694 pass, 48 fail, 45 skip
- **Ended**: 1721 pass, 21 fail, 28 skip
- **Net**: +27 passes, -27 failures, -17 skips

## Kernel Fixes Made This Session (31 commits)

### Pager infrastructure
1. **pager_failed_pages cleared on new request** — request_pages now clears stale OP_FAIL markers so retries reach the pager
2. **BAD_STATE for detached pager clones** — unsupplied pages in clones of detached pagers return BAD_STATE instead of infinite SHOULD_WAIT loop
3. **supply_pages rejects physical/pager-backed/pinned VMOs** — validates aux VMO type and pinned state
4. **supply_pages bounds check** — validates destination and source ranges are within VMO bounds
5. **supply_pages decommits aux VMO** — transfers page ownership by decommitting source pages after copy
6. **Drain resolved page requests from port** — Port::remove_page_read_packets selectively removes resolved READ packets
7. **Pager detach requires ATTACH_VMO + MANAGE_VMO** — both rights checked, not just ATTACH_VMO
8. **set_modified on vmo_write** — pager-backed VMOs get modified flag set even on partial writes

### COW / VMO attribution
9. **share_count decrement on zero** — zero marker insertion uses decrement_parent_share_counts (not release_unwanted which moves pages)
10. **Don't COW-fork in intermediate unidirectional nodes** — when a child walks through an intermediate unidirectional node, pass Ref through without forking
11. **Decommit unmap from page tables** — range_change(Unmap) on all mappings after decommitting pages
12. **Propagate decommit unmap to children** — VmObject children notified to invalidate PTEs on decommit_clean_pages
13. **Update children's parent_limit on parent resize** — unidirectional children see zeros for truncated pages instead of OUT_OF_RANGE

### Rights / ABI
14. **MANAGE_VMO right bit 24 (was 25)** — matched Fuchsia's ZX_RIGHT_MANAGE_VMO
15. **consts.rs rights alignment** — OP_CHILDREN, RESIZE, ATTACH_VMO, MANAGE_VMO all fixed

### Debug registers (x86_64)
16. **DR7 always-1 bits on read** — OR 0x700 (bits 8-10) matching Intel SDM
17. **DR6 reserved bits on read** — bits 4-11 always 1, bit 12 always 0
18. **DR7 write mask** — clear reserved bits 11-15 (0xFFFF_07FF mask)

### x86_64 context switch
19. **Zero FS/GS/DS/ES segment selectors** — zero all four data segment selectors before entering user mode, using swapgs to preserve kernel GS base

### VMAR operations
20. **op_range alignment validation** — reject unaligned address/size and zero size
21. **op_range child VMAR scoping** — COMMIT/DECOMMIT/ZERO/PREFETCH cannot traverse child VMARs

### BTI / IOMMU
22. **IOMMU map_contiguous page index** — fixed byte offset vs page index in commit_page call
23. **IOMMU bounds check error code** — OUT_OF_RANGE instead of INVALID_ARGS
24. **PMT cleanup on addrs_count mismatch** — unpin and release PMT when encoded addresses don't match caller's count
25. **release_pmt made public** — needed for PMT cleanup from syscall layer

### Handle table
26. **Sort handle entries by KOID** — approximates insertion order for ZX_INFO_HANDLE_TABLE

### Error handling
27. **Restore INVALID_ARGS→ACCESS_DENIED in vmo_read** — guarded_user_copy faults on permission-denied pages report ACCESS_DENIED

### Skip list / test infrastructure
28-31. Skip list removals (45→28): removed 9 non-hanging pager tests, ProcessTest.KillChannelHandleCycle, RegisterContextSwitchTests (self-skips)

## 28 Skipped C++ Tests

### PagerWriteback (15) — TRAP_DIRTY multi-thread
All require multi-threaded TRAP_DIRTY pager interactions that hang.

### Pager (8) — multi-thread hangs
- 6 WaitForBlocked polling tests (verified hangs)
- 2 other hangs (EarlyWakeCloneRace, EvictionHintsOpRange)

### Other (5)
- Threads.DebugRegistersDr6* (2) — hardware debug exception delivery
- VmoClone2TestCase.DropChildrenInParallel — concurrent VMO drop race
- PortStressTest.ChannelCallWaitQueuePanicRepro — 3-thread stress
- Bti.DecommitRace — multi-thread pin/decommit race

### Removed from skip list (no longer hang)
- VmoTestCase.ParallelWriteAndDecommit
- StreamTestCase.PartialVmoDirty
- Resource.Ioports
- SchedulerProfileTest.DeadlinePiUtilizationPanicRepro

## 21 Remaining C++ Test Failures — Root Cause Analysis

### COW attribution / populated_bytes (3)
- **VmoZeroTestCase.AllocateAfterMergeMultipleChildren**: multi-child share_count after merge — complex 3+ child attribution
- **Vmar.RangeOpCommit**: populated_bytes walks parent chain, reports too many pages for unidirectional children
- **TaskGetInfoTest.SharedMemAccounting**: fractional COW for unidirectional children — share_count not incremented

### Pager clone data visibility (1)
- **Pager.CloneMightSeeIntermediateForks**: multi-level clone chain with StartTaggedPageFaultHandler — COW fork visibility through hidden nodes

### PagerWriteback dirty tracking (3)
- **DelayedDirtyZeroRange**: spurious DIRTY request in port after zero
- **ModifiedOnPartialVmoWriteTrapDirty**: TRAP_DIRTY vmo_write doesn't generate dirty requests (needs syscall-level dirty handling)
- **ResizeWritebackRepeatedNoTrapDirty**: dirty ranges after resize+writeback

### ProcessGetInfo VMO enumeration (4)
- **InfoProcessVmosSmokeTest, V1, V2, V3**: mini-process has 3 VMOs, test expects 4+ (missing vDSO/TLS VMOs)

### x86 exception handling (4)
- **Property.RegisterFsNoncanonical, RegisterGsNoncanonical**: exception RIP doesn't match faulting instruction address
- **Threads.DebugRegistersWatchpointSyscallPanicRepro**: needs ZX_EXCP_HW_BREAKPOINT exception type (we deliver General)
- **Threads.SyscallDebuggerModifyResult**: debugger exception handling for modifying syscall return

### Feature gaps (4)
- **DefaultExceptionHandlerTest.UnhandledPolicyException**: policy exception kill not implemented
- **MemoryPriorityProfile.ApplyProfile**: vmo_reclaim_disabled_bytes tracking not implemented
- **StandaloneElfLoadTests.Load**: missing cross-arch test modules (arm64, riscv64 .so files)
- **VmoTestCase.ReadWriteInvalidUserBuffer**: guarded_user_copy returns INVALID_ARGS for both unmapped and permission-denied faults — need to distinguish at fault handler level

### Architecture / allocator (2)
- **Vmar.AlignmentVmarMapTest**: 8GB VMAR allocation with 512MB alignment — address space too small
- **VmoTestCase.ReadWriteBadLen**: return value corruption (-14 becomes -10) through trap return path — likely LTO miscompilation

## Testing Commands

```bash
# Full C++ suite on ThinkPad (ssh thinkpad)
cd ~/RustroverProjects/zCore
rm -rf target/qemu-x86_64/release/kernel target/qemu-x86_64/release/boot.img \
       target/qemu-x86_64/release/x86_64-zircon.img
make core-tests CORE_TESTS_TIMEOUT=600

# Check results
P=$(grep -ac '\[       OK \]' /tmp/qemu-core-tests.log)
F=$(grep -ac '\[  FAILED  \]' /tmp/qemu-core-tests.log)
echo "Passed: $P  Failed: $F"

# Local Rust tests
cargo test -p zircon-tests --lib -- --test-threads=1
cargo test -p zircon-object -p hal-impl --lib -- --test-threads=1

# Individual C++ test (local QEMU)
make core-tests CORE_TESTS_TIMEOUT=15 'CORE_TESTS_FILTER=TestName'

# Full style check
cargo check-style
```

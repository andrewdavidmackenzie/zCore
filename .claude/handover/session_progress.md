# Session Progress — Handover Document

## Current State

### Branch: `preemptive_thread_scheduling_801` (PR #802)
### Latest Commit: `b19766e1` (fix: distinguish unmapped vs permission-denied in guarded_user_copy)

### Test Results
- **Rust zircon-tests**: 3096 pass, 0 fail, 1 ignored
- **C++ core-tests (x86_64 QEMU, ThinkPad)**: 1722 pass, 20 fail, 28 skip (estimated — last full ThinkPad run was 1721/21/28 before ReadWriteInvalidUserBuffer fix)
- **CI**: All green except Unit Test (macos-latest) — pre-existing flaky `cancel_key_during_match_race` test

### Session Delta
- **Started**: 1694 pass, 48 fail, 45 skip
- **Ended**: ~1722 pass, ~20 fail, 28 skip
- **Net**: +28 passes, -28 failures, -17 skips

## Kernel Fixes Made This Session (33 commits total)

### Pager infrastructure (8 fixes)
1. **pager_failed_pages cleared on new request** — request_pages clears stale OP_FAIL markers
2. **BAD_STATE for detached pager clones** — unsupplied pages in clones of detached pagers return BAD_STATE
3. **supply_pages rejects physical/pager-backed/pinned VMOs** — validates aux VMO type
4. **supply_pages bounds check** — validates destination and source ranges
5. **supply_pages decommits aux VMO** — transfers page ownership by decommitting source
6. **Drain resolved page requests from port** — Port::remove_page_read_packets selectively removes resolved READ packets
7. **Pager detach requires ATTACH_VMO + MANAGE_VMO** — both rights checked
8. **set_modified on vmo_write** — pager-backed VMOs get modified flag on partial writes

### COW / VMO attribution (5 fixes)
9. **share_count decrement on zero** — uses decrement_parent_share_counts (not release_unwanted which moves pages)
10. **Don't COW-fork in intermediate unidirectional nodes** — when a child walks through an intermediate, pass Ref through without forking
11. **Decommit unmap from page tables** — range_change(Unmap) on all mappings after decommitting
12. **Propagate decommit unmap to children** — VmObject children notified to invalidate PTEs
13. **Update children's parent_limit on parent resize** — unidirectional children see zeros for truncated pages

### Rights / ABI (2 fixes)
14. **MANAGE_VMO right bit 24** (was 25) — matched Fuchsia's ZX_RIGHT_MANAGE_VMO
15. **consts.rs rights alignment** — OP_CHILDREN, RESIZE, ATTACH_VMO, MANAGE_VMO fixed

### Debug registers x86_64 (3 fixes)
16. **DR7 always-1 bits on read** — OR 0x700 (bits 8-10)
17. **DR6 reserved bits on read** — bits 4-11 always 1, bit 12 always 0
18. **DR7 write mask** — clear reserved bits 11-15 (0xFFFF_07FF mask)

### x86_64 context switch (1 fix)
19. **Zero FS/GS/DS/ES segment selectors** — zero all four before entering user mode, using swapgs to preserve kernel GS base

### VMAR operations (2 fixes)
20. **op_range alignment validation** — reject unaligned address/size and zero size
21. **op_range child VMAR scoping** — COMMIT/DECOMMIT/ZERO/PREFETCH cannot traverse child VMARs

### BTI / IOMMU (4 fixes)
22. **IOMMU map_contiguous page index** — fixed byte offset vs page index in commit_page
23. **IOMMU bounds check error code** — OUT_OF_RANGE instead of INVALID_ARGS
24. **PMT cleanup on addrs_count mismatch** — unpin and release PMT on failure
25. **release_pmt made public** — needed for PMT cleanup from syscall layer

### Handle table (1 fix)
26. **Sort handle entries by KOID** — approximates insertion order for ZX_INFO_HANDLE_TABLE

### Fault handling (1 fix)
27. **Distinguish unmapped vs permission-denied in guarded_user_copy** — new Error::PermissionDenied variant, x86 PRESENT bit detection. Unmapped → NOT_FOUND, permission-denied → ACCESS_DENIED

### Skip list (4 commits)
28-31. Skip list reductions (45→28)

### CI fixes (2 commits)
32. **Rust test assertions** for DR6/DR7 reserved bits
33. **Clippy unnecessary_cast** in port.rs

## 28 Skipped C++ Tests

### PagerWriteback (15) — TRAP_DIRTY multi-thread hangs
### Pager (8) — multi-thread hangs (6 WaitForBlocked + 2 other)
### Other (5) — debug register exceptions, concurrent VMO drop, port stress, BTI race

## 20 Remaining C++ Test Failures — Root Cause Analysis

### COW attribution / populated_bytes (3)
- **VmoZeroTestCase.AllocateAfterMergeMultipleChildren**: 3+ child share_count after merge
- **Vmar.RangeOpCommit**: populated_bytes walks parent chain for unidirectional children
- **TaskGetInfoTest.SharedMemAccounting**: fractional COW for unidirectional children

### Pager clone data visibility (1)
- **Pager.CloneMightSeeIntermediateForks**: multi-level clone with StartTaggedPageFaultHandler

### PagerWriteback dirty tracking (3)
- **DelayedDirtyZeroRange**: spurious DIRTY request after zero
- **ModifiedOnPartialVmoWriteTrapDirty**: TRAP_DIRTY vmo_write needs syscall-level dirty handling
- **ResizeWritebackRepeatedNoTrapDirty**: dirty ranges after resize+writeback

### ProcessGetInfo VMO enumeration (4)
- **InfoProcessVmosSmokeTest, V1, V2, V3**: mini-process has 3 VMOs, test expects 4+

### x86 exception handling (4)
- **Property.RegisterFsNoncanonical, RegisterGsNoncanonical**: exception RIP mismatch
- **Threads.DebugRegistersWatchpointSyscallPanicRepro**: needs ZX_EXCP_HW_BREAKPOINT
- **Threads.SyscallDebuggerModifyResult**: debugger exception handling

### Feature gaps (3)
- **DefaultExceptionHandlerTest.UnhandledPolicyException**: policy exception kill
- **MemoryPriorityProfile.ApplyProfile**: vmo_reclaim_disabled_bytes tracking
- **StandaloneElfLoadTests.Load**: missing cross-arch test modules

### Architecture / compiler (2)
- **Vmar.AlignmentVmarMapTest**: 8GB VMAR allocation — address space layout
- **VmoTestCase.ReadWriteBadLen**: return value corruption through trap path (LTO)

## Key Architectural Knowledge

### guarded_user_copy fault distinction
The `guarded_user_copy` now distinguishes unmapped (InvalidPointer → INVALID_ARGS → NOT_FOUND) from permission-denied (PermissionDenied → ACCESS_DENIED) faults. On x86, this uses the PRESENT bit (bit 0) of the page fault error code. The `user_copy_check_fault_with_perm` function passes this through per-CPU state to `user_copy_leave()` which returns `(fault_addr, is_permission_fault)`.

### Unidirectional child COW chain
When a unidirectional child B is created from another unidirectional child A (via `A.Clone()` → `parent.create_child_unidirectional()`), B's VMObjectPaged parent is A (not the root). When B does commit_page(WRITE), it walks A → root. If A is also unidirectional and sees WRITE + Ref, it must NOT fork locally — only the leaf (B) should fork. The `child.strong_count() > 0` check in `commit_page_internal` handles this.

### Segment selector zeroing
Before entering user mode on x86_64, we zero DS/ES/FS/GS selectors. FS and GS zeroing must preserve the kernel GS base via swapgs: `swapgs; mov gs,0; swapgs`. The trapframe's `wrfsbase`/`wrgsbase` then set the correct user bases.

### Port packet cleanup after supply
When `supply_pages` resolves outstanding page requests, resolved `ZX_PAGER_VMO_READ` packets are removed from the port queue via `Port::remove_page_read_packets`. This prevents stale requests from confusing the pager's `WaitForPageRead`.

## Testing Commands

```bash
# Pre-push checks (ALWAYS run before pushing)
cargo check-style

# Full C++ suite on ThinkPad
ssh thinkpad
cd ~/RustroverProjects/zCore
rm -f target/qemu-x86_64/release/kernel
make core-tests CORE_TESTS_TIMEOUT=600

# Check results
P=$(grep -ac '\[       OK \]' /tmp/qemu-core-tests.log)
F=$(grep -ac '\[  FAILED  \]' /tmp/qemu-core-tests.log)
echo "Passed: $P  Failed: $F"

# Local Rust tests
cargo test -p zircon-tests --lib -- --test-threads=1

# Individual C++ test (local QEMU)
make core-tests CORE_TESTS_TIMEOUT=15 'CORE_TESTS_FILTER=TestName'
```

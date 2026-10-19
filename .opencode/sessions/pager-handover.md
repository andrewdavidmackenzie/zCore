# Pager Test Handover — Remaining 19 Tests

## Branch
`fix-pager-remaining` based on master at `1b4bd9d6` (after #740 merge).

## What's Done
Skip list went from 300 → ~128. QEMU core-tests from 524 → ~1400+ passing.
All Pager infrastructure implemented:
- Demand-paging (SHOULD_WAIT, supply_pages, wait_for_page)
- Eager-commit bypass for pager VMOs on bare-metal
- BlockedPager thread state in page fault handler + sys_vmo_read + op_range
- Batched page requests (per-run splitting around committed/waitered pages)
- Early wake (interleaved page-by-page copy in sys_vmo_read)
- Clone pager routing (find_root_pager_vmo, is_in_pager_tree)
- COW sibling page visibility (hidden node checks pager-backed sibling)
- VMAR op_range async demand-paging (multi-VMO, clone-aware)
- OP_FAIL with failed-pages tracking + one retry
- ZX_PAGER_VMO_COMPLETE on detach and VMO drop
- Resize truncation wakes pager waiters
- wait_for_page returns OUT_OF_RANGE for truncated pages
- ALWAYS_NEED/PREFETCH trigger demand-paging

## Remaining 11 Pager.* Tests

### Overlapping/Multi-thread (5 tests)
```
Pager.FailOverlapRequest
Pager.FailOverlappingRangeCommit
Pager.EarlyWakeOverlappingRequests
Pager.EarlyWakeCloneRace
Pager.PageRequestBatchingChecksAncestorPages
```
**Root cause**: These tests have multiple threads doing overlapping reads/commits.
The Fuchsia kernel tracks pending page requests per-page and absorbs
overlapping requests (the second thread doesn't generate a new port packet
if the range is already covered by another thread's request). Our kernel
sends separate requests per thread.

**What's needed**:
- Track "pending request ranges" in the VMO (not just per-page waiters)
- When a new request overlaps an existing pending range, suppress it
- When pages are supplied, check if any overlapping waiters can be woken
- `FailOverlapRequest` also needs partial-supply + partial-fail semantics
  where the kernel retries unsupplied pages after a partial supply
- `PageRequestBatchingChecksAncestorPages` needs the commit batching to
  check ancestor VMOs (hidden node + root) for committed pages before
  sending requests — a 3-level clone tree (root → child → grandchild)

**Fuchsia reference**: `vm_cow_pages.cc` `LookupCursor::ReadRequest()` —
batches requests up to the first present page in the owner's page list.
The "owner" walks up through hidden nodes to find the root pager VMO.

### Suspend During Pager Wait (3 tests)
```
Pager.SuspendCommitTest
Pager.SuspendReadTestVmar
Pager.SuspendReadTestVmo
```
**Root cause**: These tests suspend a thread while it's blocked on a pager
wait, resume it, then expect the thread to re-send the page request.

**What's needed**:
- When a pager-blocked thread is suspended (`zx_task_suspend`), it should
  be removed from the pager wait
- When resumed, the page request should be re-sent and the thread should
  re-enter `wait_for_page`
- The `SuspendCommitTest` also involves resize during suspend

**Fuchsia reference**: Thread suspend interacts with the page fault
handler's retry loop. On resume, the CPU re-executes the faulting
instruction, triggering a new page fault → new pager request.

### Resize + BTI Pin (1 test)
```
Pager.ResizeBlockedPin
```
**Root cause**: Needs BTI (Bus Transaction Initiator) pin during a pager
wait. The test pins pages while they're being demand-paged, then resizes.

**What's needed**:
- Full IOMMU/BTI pin implementation during pager waits
- This is deep hardware infrastructure — may be lowest priority

### Data Mismatch (1 test)
```
Pager.CloneReadFromParentTestVmar
```
**Root cause**: The Vmo variant PASSES (data correct via zx_vmo_read).
The Vmar variant FAILS with data mismatch — the page table mapping
points to an ELF code page instead of the supplied pager data.

The COW sibling check in `commit_page_internal` returns `Ref(paddr)` from
the root's frame. The paddr value is correct for `zx_vmo_read` (which
copies data from the phys page) but the VMAR mapping sees wrong data.

**Investigation needed**:
- Check if `handle_page_fault` maps the correct paddr returned by
  `commit_page`
- Verify the page table entry after mapping
- May be a TLB or page table coherency issue on bare-metal

### Port Race (1 test)
```
Pager.RacyPortDequeue
```
**Root cause**: Tests a race condition in port packet dequeuing. May need
atomic operations or locking improvements in the port implementation.

## Remaining 8 PagerProcess.* Tests
```
PagerProcess.KillBlockedUserVmoRead
PagerProcess.KillBlockedUserPageFault
PagerProcess.KillBlockedUsercopy
PagerProcess.KillBlockedUsercopyCaptureFaults
PagerProcess.KillAfterSuspendVmoRead
PagerProcess.KillAfterSuspendPageFault
PagerProcess.KillAfterSuspendUsercopy
PagerProcess.KillAfterSuspendUsercopyCapture
```
**Root cause**: These tests kill a process while a thread is blocked on
a pager page fault. The kill should unblock the thread and clean up.
The "AfterSuspend" variants first suspend the thread, then kill.
The "CaptureFaults" variants use `ZX_VMO_OP_MAP_RANGE` with
`ZX_VM_MAP_RANGE` to capture faults.

**What's needed**:
- Process kill must wake pager-blocked threads
- Thread kill during `wait_for_page` should return an error
- The "usercopy" variants test `zx_channel_write` with a buffer in
  a pager-backed mapping — the kernel copy faults and should be killed

**Current state**: `VmObject::drop` already calls
`fail_pager_requests(BAD_STATE)`. Process kill calls `Thread::kill()`
which sets `killed=true` and wakes the killer channel. But the
`wait_for_page` future might not be canceled by the kill.

## Key Files
- `zCore/zircon-object/src/vm/vmo/mod.rs` — VmObject, request_pages,
  wait_for_page, pager_waiters, pager_failed_pages
- `zCore/zircon-object/src/vm/vmo/paged.rs` — commit_page_internal,
  COW sibling check, hidden node handling
- `zCore/zircon-object/src/vm/vmar.rs` — handle_page_fault, eager commit
  bypass, find_pager_vmos_in_range
- `zCore/zircon-object/src/signal/pager.rs` — Pager, supply_pages,
  detach_vmo, op_range
- `zCore/zircon-syscall/src/vmo.rs` — sys_vmo_read (early wake),
  sys_vmo_op_range_async (per-run batching, clone routing)
- `zCore/zircon-syscall/src/vmar.rs` — sys_vmar_op_range (async
  demand-paging for multi-VMO ranges)
- `zCore/zircon-loader/src/zircon.rs` — handler_user_trap (SHOULD_WAIT
  handling, BlockedPager state, clone pager routing)
- `zCore/zircon-object/src/signal/port_packet.rs` — ZX_PAGER_VMO_*
  constants

## C++ Test Source
Cached at `/tmp/pager.cc` and `/tmp/userpager.cc`. If missing:
```bash
curl -s 'https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/system/utest/core/pager/pager.cc?format=TEXT' | base64 -d > /tmp/pager.cc
curl -s 'https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/system/utest/core/pager/userpager.cc?format=TEXT' | base64 -d > /tmp/userpager.cc
```

Fuchsia kernel VMO source:
```bash
curl -s 'https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/vm/vm_cow_pages.cc?format=TEXT' | base64 -d > /tmp/vm_cow_pages.cc
```

## Quick Test Command
```bash
make core-test-run T=Pager.TestName CORE_TEST_TIMEOUT=15
```

## Priority Order
1. **PagerProcess.KillBlocked*** — likely fixable by waking pager waiters
   on process/thread kill (8 tests)
2. **Suspend*** — suspend/resume during pager wait (3 tests)
3. **CloneReadFromParentTestVmar** — data mismatch debugging (1 test)
4. **FailOverlap/EarlyWakeOverlapping** — overlapping request tracking
   (3 tests)
5. **PageRequestBatchingChecksAncestorPages** — ancestor page check (1 test)
6. **EarlyWakeCloneRace** — clone + early wake race (1 test)
7. **RacyPortDequeue** — port race (1 test)
8. **ResizeBlockedPin** — BTI pin (1 test, lowest priority)

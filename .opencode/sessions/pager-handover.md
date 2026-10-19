# Pager Test Handover — Updated Status

## Branch
`fix-pager-remaining` — PR #745

## Progress Summary
- Started: 19 remaining pager tests
- Fixed: 5 tests now passing (4 PagerProcess.Kill + 1 EarlyWakeOverlapping)
- Infrastructure added: blocking_run killable/suspendable, outstanding request overlap tracking

## Fixed Tests (5 total)

### PagerProcess Kill Tests (4 tests) — via blocking_run
```
PagerProcess.KillBlockedUserVmoRead       ✅
PagerProcess.KillBlockedUserPageFault     ✅
PagerProcess.KillAfterSuspendVmoRead      ✅
PagerProcess.KillAfterSuspendPageFault    ✅
```
**Fix**: Wrapped all `wait_for_page()` calls in `blocking_run()` (page fault handler)
and `blocking_run_suspendable()` (syscalls) so Thread::kill() can cancel pager
waits via the killer channel. Matches Fuchsia's `Interruptible::Yes` semantics.

### EarlyWakeOverlappingRequests (1 test) — via overlap absorption
```
Pager.EarlyWakeOverlappingRequests        ✅
```
**Fix**: Added `OutstandingPageRequest` tracking in VMO. When a new request's
start page falls within an existing outstanding request, it's absorbed (no
new port packet). Overlap waiters are woken when pages are supplied/failed.

## Still Skipped (14 tests)

### Usercopy Pager Tests (4 tests) — needs demand-paging in guarded_user_copy
```
PagerProcess.KillBlockedUsercopy
PagerProcess.KillBlockedUsercopyCaptureFaults
PagerProcess.KillAfterSuspendUsercopy
PagerProcess.KillAfterSuspendUsercopyCapture
```
**Root cause**: `guarded_user_copy` uses synchronous fault recovery (recovery
label + error flag). When it faults on a pager-backed mapping, it returns
`Error::InvalidPointer` immediately instead of waiting for the pager. Fixing
this requires making the guarded copy path async/demand-paging-aware.

### Suspend Tests (3 tests) — needs preemptive scheduling
```
Pager.SuspendCommitTest
Pager.SuspendReadTestVmar
Pager.SuspendReadTestVmo
```
**Root cause**: These tests suspend a thread during a pager wait, resume it,
and expect the thread to re-send the page request. The `blocking_run_suspendable`
infrastructure is in place (suspend_interrupt channel fires), but the cooperative
scheduler can't properly pause a thread mid-syscall. `wait_for_resume()` exists
but the timing doesn't work — by the time the pager thread processes the suspend
notification, the test has already called resume. The yield-based polling loop
busy-loops without making progress.

**Infrastructure in place**:
- `suspend_interrupt: Option<Sender<()>>` in ThreadInner
- `blocking_run_suspendable()` method on CurrentThread
- `wait_for_resume()` method on CurrentThread
- `is_suspended()` method on Thread
- Suspend/resume retry loops in sys_vmo_read, sys_vmar_op_range

### VMAR Data Mismatch (1 test)
```
Pager.CloneReadFromParentTestVmar
```
**Root cause**: The VMO variant passes (data correct via zx_vmo_read). The VMAR
variant reads `\x7FELF` (ELF header) from the page table mapping instead of the
supplied pager data. The `commit_page` sibling check returns the correct paddr
(verified by VMO read working), but something is wrong with how the page table
maps it. Likely a stale mapping or incorrect paddr in the bare-metal page table.
Needs QEMU debugging with logging to trace the actual paddr values through
handle_page_fault → commit_page → pg_table.map.

### Overlapping Request Tests (3 tests) — needs deeper Fuchsia semantics
```
Pager.FailOverlapRequest
Pager.FailOverlappingRangeCommit
Pager.EarlyWakeCloneRace
```
**Root cause**: These need Fuchsia's full page request state machine:
1. **Early wake continuation**: When a thread is early-woken by partial supply,
   it re-enters `GetPages` and the system recognizes the existing request as a
   `SameRequest` continuation (adjusts `wake_offset_`, returns SHOULD_WAIT).
2. **Per-overlap error tracking**: Each overlap has its own `complete_status_`.
   When pages are failed, the error is stored on overlaps whose range starts at
   the failed offset. When the primary completes, overlaps get their individual
   error status.
3. **Retry after complete**: After a request completes, the thread re-enters
   `GetPages`. If pages are still missing (e.g., failed pages), a **new** request
   is generated. The test checks for this second request.
4. **Clone page lookup after early wake** (EarlyWakeCloneRace): After early wake,
   the thread's re-scan must check clone-forked pages (from zero+write during
   the wait). This changes the request range.

### Ancestor Page Batching (1 test)
```
Pager.PageRequestBatchingChecksAncestorPages
```
**Root cause**: The request batching in sys_vmo_read sends one request for
the full range of missing pages. It doesn't check ancestor VMOs in the clone
tree for already-committed pages. In Fuchsia, `LookupCursor::ReadRequest`
stops at the first present page in the owner's page list, which includes
ancestor pages. Fix: split batched requests around pages that exist in
ancestor VMOs.

### Port Race (1 test) — needs multi-CPU or preemptive scheduling
```
Pager.RacyPortDequeue
```
**Root cause**: Stress test with `arch::Yield()` spin loop. On single-CPU
cooperative scheduler, threads never interleave. The test runs up to 1000
iterations racing pager destruction against port dequeue.

### BTI Pin (1 test) — lowest priority
```
Pager.ResizeBlockedPin
```
**Root cause**: Needs full IOMMU/BTI pin implementation during pager waits.

## Key Source Files
- `zCore/zircon-object/src/vm/vmo/mod.rs` — VmObject, request_pages,
  wait_for_page, OutstandingPageRequest, overlap absorption
- `zCore/zircon-object/src/vm/vmo/paged.rs` — commit_page_internal,
  COW sibling check
- `zCore/zircon-object/src/vm/vmar.rs` — handle_page_fault, mapping
- `zCore/zircon-object/src/signal/pager.rs` — supply_pages, op_range
- `zCore/zircon-object/src/task/thread.rs` — blocking_run,
  blocking_run_suspendable, wait_for_resume, suspend_interrupt
- `zCore/zircon-syscall/src/vmo.rs` — sys_vmo_read pager wait + retry
- `zCore/zircon-syscall/src/vmar.rs` — sys_vmar_op_range pager wait
- `zCore/zircon-loader/src/zircon.rs` — handler_user_trap page fault

## Fuchsia Reference
- `/tmp/page_source.cc` — PageSource: outstanding requests, overlap,
  early wake, resolve, complete
- `/tmp/vm_cow_pages.cc` — VmCowPages: LookupCursor::ReadRequest batching
- `/tmp/pager.cc` — C++ test source
- `/tmp/pager-process.cc` — PagerProcess test source

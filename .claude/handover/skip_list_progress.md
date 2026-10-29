# Skip List Progress — Handover Document

## Current State (after PR #800)

### Skip list: 372 → 88 (284 removed, 76%)

### Test counts
- Rust core_tests: 1778 (1:1 with C++ binary)
- Rust passing: 3062
- Rust #[ignore]: 32
- C++ skip list: 88

### Audit checker
`cargo audit-tests` (or `make audit-tests`) verifies:
- 1:1 correspondence (count, doc comments, no dups, no stubs)
- No `unimplemented!()` in core_tests
- Every `#[ignore]` test is in the skip list
- Runs in CI via `cargo check-style`

## Kernel fixes made (PR #800)

1. **PAGE_SIZE consolidation** — 7 duplicate definitions → 1 in `hal/src/defs.rs`
   - Added `system_get_page_size` syscall (239) for petal/userstart
   - vDSO uses `hal::PAGE_SIZE` instead of hardcoded 4096
   - Petal binaries call `zx_system_get_page_size()` at runtime
   - Fixed hardcoded 4096 in vmo.rs, channel.rs tests

2. **OP_ZERO dirty tracking** — 3 separate fixes:
   - Clean zero markers not falsely dirtied (pre-zero snapshot)
   - Freshly-zeroed pages correctly marked as modified
   - COW clones insert zero marker without unnecessary COW fork

3. **Pager rights checks** — `create_vmo` requires ATTACH_VMO,
   `detach_vmo` requires ATTACH_VMO, `supply_pages` requires MANAGE_VMO

4. **set_stream_size modified flag** — pager VMOs marked modified on
   stream size change regardless of UNBOUNDED flag

5. **IPI wakeup on cross-CPU spawn** — newly spawned tasks on halted
   CPUs start immediately via IPI instead of waiting for timer tick

6. **Syscall-count forced preempt yield** — prevents tight polling
   loops from starving newly-spawned threads

## Remaining skip list by suite (88 total)

| Suite | Count | Root Cause |
|-------|-------|-----------|
| PagerWriteback | 28 | BTI/pin TrapDirty (14), concurrent TestThread (6), dirty range bugs (6), partial write (1), suspend (1) |
| Pager | 23 | Clone lifecycle/crash exceptions (7), multi-thread overlap (4), EarlySupply spurious request (2), Suspend (3), other (7) |
| PortStressTest | 12 | Multi-thread stress races in cooperative executor |
| PagerProcess | 8 | Process + page fault infrastructure |
| Threads | 7 | Debug registers / exception delivery |
| VmoClone2TestCase | 4 | populated_bytes (2), progressive hang (1), 16K pages (1) |
| Snapshot | 2 | Zero-range page tracking |
| Other | 4 | Bti.DecommitRace, StreamTestCase, Resource, ProcessTest |

## Infrastructure needed for remaining tests

### 1. WaitForCrash / exception delivery (unblocks ~7 Pager tests)
Tests like `DecommitOnDetachTestVmar`, `CloneDetachTest` use
`TestThread::WaitForCrash(addr, error)` which expects a fatal page
fault exception with specific `synth_code` on the exception channel.
zCore needs to deliver proper exceptions when pager VMO access fails
(after detach/close).

### 2. Multi-thread overlap commit (unblocks ~4 Pager tests)
`OverlapCommitSupplyTest`, `FailMultipleCommits` create multiple
threads that commit overlapping ranges. The C++ `WaitForBlocked()`
pattern works for single-thread tests but fails for overlap scenarios.
Root cause: when a thread's commit falls within an existing outstanding
request (overlap absorption), the thread blocks on the overlap waiter
but may not reach `BlockedPager` state visible to polling.

### 3. Clone resize/write probe (unblocks ~4 Pager tests)
`CloneResizeParentOK`, `CloneShrinkGrowParent` use `probe_for_read`/
`probe_for_write` on mapped clones after parent resize. Need correct
VMAR page fault handling + resize semantics for pager-backed clones.

### 4. PagerWriteback BTI/pin TrapDirty (14 tests)
Use `ThreadedPagePinner` creating threads that pin pages via BTI.
Need working BTI pin in concurrent thread context with TRAP_DIRTY.

### 5. PagerWriteback concurrent TestThread mapping writes (6 tests)
Use TestThread with mapped VMAR writes triggering TRAP_DIRTY faults.
Need VMAR dirty fault → pager DIRTY request path.

### 6. EarlySupply spurious page request (2 tests)
`sys_vmo_read` generates a page request for already-supplied pages.
Need investigation into why `committed_pages_in_range` returns 0
for a page that was supplied via `supply_pages`.

## Testing commands

```bash
# Run audit
cargo audit-tests

# Run all Rust tests
cargo test -p zircon-tests --lib -- --test-threads=1

# Run specific suite on QEMU
make core-tests CORE_TESTS_FILTER='SuiteName.*' CORE_TESTS_TIMEOUT=60

# Run specific test on QEMU
make core-tests CORE_TESTS_FILTER='SuiteName.TestName' CORE_TESTS_TIMEOUT=30

# Full pre-push check
make pre-push
```

## Key files

- Skip list: `tests/core-tests-skip.txt`
- C++ test reference: `tests/fuchsia-core-tests-from-binary.txt`
- Audit checker: `tools/xtask/src/audit_tests.rs`
- Pager tests: `zCore/zircon-tests/src/core_tests/pager.rs`
- VMO dirty tracking: `zCore/zircon-object/src/vm/vmo/mod.rs`
- Pager kernel object: `zCore/zircon-object/src/signal/pager.rs`
- Pager syscalls: `zCore/zircon-syscall/src/pager.rs`
- VMO op_range: `zCore/zircon-syscall/src/vmo.rs`
- Executor runtime: `zCore/executor/src/runtime.rs`
- Thread scheduling: `zCore/zircon-loader/src/zircon.rs`

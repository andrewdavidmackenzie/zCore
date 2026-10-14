# Core Tests Functional Equivalence Audit

Audit date: October 2026
Compared: Rust `core_tests/` against Fuchsia C++ `core-tests-standalone` source

## Grading Scale

- **FULL**: Exercises same kernel code paths and makes equivalent assertions
- **PARTIAL**: Tests same concept but misses edge cases, uses different API layer,
  or simplifies the test logic
- **STUB**: Has a function body but doesn't exercise the intended behavior
- **MISSING**: No Rust equivalent exists

## Summary by Suite

| Suite | C++ Tests | FULL | PARTIAL | STUB | MISSING |
|-------|-----------|------|---------|------|---------|
| Channel (all) | 88 | 33 | 40 | 6 | 6 |
| Socket | 67 | 32 | 34 | 1 | 0 |
| VMO (all) | 193 | 105 | 62 | 6 | 13 |
| Port | 40 | 13 | 25 | 0 | 2 |
| Timer | 16 | 0 | 12 | 2 | 2 |
| Pager (all) | 209 | 15 | 65 | 129 | 0 |
| Thread | 44 | 3 | 26 | 14 | 1 |
| Process | 30 | 8 | 15 | 2 | 5 |
| Job | 24 | 16 | 5 | 0 | 3 |
| Fifo | 34 | 15 | 5 | 0 | 8 |
| EventPair | 15 | 1 | 1 | 0 | 13 |
| Resource | 36 | 16 | 5 | 3 | 12 |
| Futex | 28 | 14 | 1 | 1 | 5 |
| ObjectWait | 12 | 7 | 3 | 0 | 0 |
| Property | 20 | 14 | 6 | 0 | 0 |
| BTI | 13 | 7 | 3 | 0 | 2 |
| Stream | 33 | 14 | 5 | 0 | 8 |
| Interrupt | 21 | 10 | 1 | 3 | 2 |
| **TOTAL** | **923** | **323 (35%)** | **314 (34%)** | **167 (18%)** | **82 (9%)** |

Note: Remaining suites not audited in detail (~855 tests): object-info (236),
vmar (62), profile (34), clock (5), debuglog (9), counter (18), etc.
These follow similar patterns to the audited suites.

## Systemic Findings

### 1. No Demand-Paging Tests (~60% of Pager suite)

The C++ pager tests use real Zircon threads that block on page faults,
with the pager servicing requests via port packets. Rust tests operate
synchronously: `supply_pages()` then `vmo.read()`. This means:
- No page fault delivery via port packets
- No WaitForPageRead/WaitForPageDirty verification
- No concurrent multi-thread access patterns

**Impact**: The pager suite has 391 Rust test functions but only ~7%
achieve FULL equivalence. The C++ binary (via `make core-tests`) is
the authoritative test for demand-paging paths.

### 2. Cooperative Scheduler Limits Concurrency Tests

All stress/race tests (PortStressTest, thread suspend-during-blocking,
concurrent reads, process kill races) are single-threaded in Rust.
They verify API correctness but cannot exercise concurrency paths.

**Affected suites**: Port (12 stress tests), Thread (14 stubs),
Process (race stress), Channel (3 missing concurrent tests).

### 3. Object-Level vs Syscall-Level Testing

Many Rust tests operate at the kernel-object API level (e.g.,
`Channel::create()`, `VmObject::read()`) rather than through the
syscall dispatch layer (`sys_channel_create()`, `sys_vmo_read()`).
This means:
- Handle rights enforcement is untested
- Bad pointer/buffer validation is untested
- Error code mapping (kernel→userspace) is untested

### 4. Snapshot Tests Are All Identical (37/37)

Every Snapshot test in `pager.rs` shares the exact same 13-line body:
create VMO, supply 1 page, create child, read 9 bytes. None exercise
snapshot-specific semantics (SNAPSHOT_MODIFIED, COW page release,
zero-range isolation, memory accounting).

### 5. Timer Tests Never Achieve FULL Grade

Rust `Timer::set()` takes `Duration` (unsigned), making negative
deadlines impossible. All C++ tests using `ZX_TIME_INFINITE_PAST` or
negative deadlines are approximated with `Duration::from_nanos(0)`.
No `get_info`/`wait_one` verification pattern exists.

### 6. EventPair Is Critically Under-Tested (2/15)

Only 2 of 15 C++ tests ported. Missing: PEER_CLOSED lifecycle, handle
duplication, rights enforcement, invalid signal bits, signal+clear
cycles.

### 7. Exception Infrastructure Absent

14 Thread tests and 2 Channel tests require exception channels
(`zx_task_create_exception_channel`), exception handler protocol, and
`ZX_EXCP_*` types. All are stubs.

## Strongest Suites

- **Job**: 67% FULL — hierarchy, kill chains, critical processes, policy V2
- **Futex**: 50% FULL — multi-threaded wake, requeue, owner validation
- **ObjectWait**: 58% FULL — signal semantics, timeout, bad handle
- **Property**: 70% FULL — name get/set across all object types
- **BTI**: 54% FULL — pin/unpin, contiguous, quarantine lifecycle
- **VmoSignal**: 100% FULL (3/3 tests)

## Weakest Suites

- **EventPair**: 7% FULL (1/15) — needs complete rewrite
- **Timer**: 0% FULL — needs `get_info` pattern and negative deadline support
- **Thread**: 7% FULL — blocked by exception infrastructure
- **Pager**: 7% FULL — blocked by demand-paging infrastructure

## Recommendations

### High Priority (kernel correctness)

1. **Add EventPair tests**: 13 missing tests, mostly straightforward
   signal/PEER_CLOSED/rights checks. No infrastructure blockers.

2. **Fix Timer tests**: Add `get_info` verification after every set/cancel.
   Use `i64` deadlines at syscall layer to test negative values.

3. **Fix Snapshot tests**: Each of the 37 tests needs its own unique body
   matching the C++ test logic. Currently all are copy-paste.

4. **Add missing FIFO validation tests**: Create boundary checks, bad
   pointer rejection, rights enforcement.

### Medium Priority (test quality)

5. **Add syscall-layer counterparts**: For object-level tests that claim
   to test rights/validation but operate above the rights check layer.

6. **Use exact signal mask equality**: Replace `signal().contains(X)` with
   `assert_eq!(signal(), expected_mask)` for complete signal state
   verification.

7. **Fix VmoTransferData tests**: Currently simulated with read+write+zero
   instead of the actual `zx_vmo_transfer_data` syscall.

### Lower Priority (infrastructure-blocked)

8. **Exception channels**: Requires kernel implementation before 14 Thread
   tests and 2 Channel tests can be unblocked.

9. **Demand-paging test infrastructure**: Build a Rust equivalent of the
   C++ `UserPager` + `TestThread` classes for async page fault testing.

10. **Preemptive scheduler**: Required for all stress/race tests to be
    meaningful. Currently validated via `make core-tests` on QEMU.

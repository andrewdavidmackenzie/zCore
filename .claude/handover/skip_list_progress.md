# Skip List Progress — Handover Document

## Current State (after PRs #796, #797, #798)

### Skip list: 520 → 372 (148 removed, 28%)

### Test counts
- Rust core_tests: 1778 (1:1 with C++ binary)
- Rust passing: 3053
- Rust #[ignore]: 44
- C++ skip list: 372

### Audit checker
`cargo audit-tests` (or `make audit-tests`) verifies:
- 1:1 correspondence (count, doc comments, no dups, no stubs)
- No `unimplemented!()` in core_tests
- Every `#[ignore]` test is in the skip list
- Runs in CI via `cargo check-style`

## Remaining skip list by suite

| Suite | Count | Root Cause | Issue |
|-------|-------|-----------|-------|
| PagerWriteback | 179 | Demand-paging pipeline | Needs page fault → pager request → supply → resume |
| Pager | 135 | Demand-paging pipeline | Same as above |
| Threads | 7 | Exception delivery, debug exceptions, INTR_RETRY | #799 |
| VmoClone2 | 13 | populated_bytes / fractional page attribution | Kernel VMO accounting |
| Bti | 13 | BTI pin crash (8/13 pass, 5 crash) | BTI implementation gaps |
| PortStressTest | 12 | Multi-threaded stress races | Cooperative executor limitation |
| PagerProcess | 8 | Demand-paging | Same as Pager |
| Snapshot | 2 | Zero-range page tracking | Specific kernel fix |
| Others | 3 | Individual issues | |

## 7 Remaining Threads tests (Issue #799)

All require real thread execution or exception delivery:

| Test | Needs |
|------|-------|
| BadSyscallCaller | Exception channel delivery for invalid syscall |
| ExitViaException | Exception channel delivery |
| DebugRegistersDr6CorrectAfterBreakpointRemoval | Hardware breakpoint → debug exception |
| DebugRegistersDr6ResetOnDebugException | Hardware debug exception |
| DebugRegistersWatchpointSyscallPanicRepro | Watchpoint during syscall |
| SyscallDebuggerModifyResult | Modify rax during suspend → honor on resume |
| ReadingGeneralRegisterState | rflags RF bit (bit 16) |

### Blocked on: executor integration into libos (#799)

The bare-metal executor has libos support (`switch_libos.S` for x86_64 and aarch64, no privileged instructions). But wiring it into the test harness requires replacing `#[async_std::test]` because `run_until_idle()` uses `switch()` which must run on the same OS thread as the tasks.

## Kernel fixes made

1. ZX_INFO_THREAD_STATS BAD_STATE for dead threads
2. Entry/stack address validation in sys_thread_start
3. Process marked Running on thread_start (via ensure_running in syscall layer)
4. FloatPoint register state read/write on x86_64 (160 bytes, st[] at offset 32)
5. abi_reg delivered via r15 (x86_64) / x18 (aarch64)
6. IOPL=0 for user threads (was IOPL=3)
7. INTR_RETRY (-6) set in return register when suspending blocked thread
8. DR6 default 0xFFFF0FF0, masking (bits 4-11 always 1, bit 12 always 0, 32-bit)
9. DR7 masking (set LE/GE bits 8-10, clear reserved 11-15, 32-bit)
10. DR0-DR3 address validation (reject kernel-space/non-canonical)
11. ThreadState::is_blocked() helper

## Next steps by priority

### 1. VmoClone2 (13 tests) — most tractable
The `populated_bytes` / `populated_scaled_bytes` fields in VMO info need fractional page attribution for COW clones. The tests hang polling `populated_bytes` because zCore doesn't implement the attribution algorithm that Fuchsia uses for shared pages.

### 2. Bti (13 tests) — partial progress
8/13 already pass. The 5 that crash have specific BTI pin/unpin issues. Run individually on QEMU to identify exact failures.

### 3. Snapshot (2 tests) — specific fix
`ZeroRangeLeftInSnapshotNoPagesInParent` and `ZeroRangeLeftInSnapshotNoPagesInParentChain` — zero-range operations on snapshot VMOs when parent has no committed pages.

### 4. PortStressTest (12 tests) — cooperative executor limitation
Multi-threaded stress tests that need real concurrent execution. May work with the executor integration (#799).

### 5. Pager/PagerWriteback (322 tests) — deep kernel work
Requires the demand-paging page fault pipeline:
1. User thread accesses unmapped pager-backed page
2. #PF handler detects pager VMO, sends ZX_PAGER_VMO_READ packet to port
3. Thread blocks in ThreadState::BlockedPager
4. Pager thread calls supply_pages()
5. Blocked thread is woken, page table updated, execution resumes

The Rust pager tests bypass this by calling vmo.read() directly (no page faults). The C++ tests use real VMAR mappings + TestThread infrastructure.

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
make pre-push-quick
```

## Key files

- Skip list: `tests/core-tests-skip.txt`
- C++ test reference: `tests/fuchsia-core-tests-from-binary.txt`
- Audit checker: `tools/xtask/src/audit_tests.rs`
- Thread tests: `zCore/zircon-tests/src/core_tests/thread.rs`
- Extra thread tests: `zCore/zircon-tests/src/extra_tests/thread_extra.rs`
- Thread state read/write: `zCore/zircon-object/src/task/thread/thread_state.rs`
- Context setup: `zCore/hal-impl/src/common/context.rs`
- Syscall dispatch: `zCore/zircon-syscall/src/task.rs`
- Executor libos: `zCore/executor/src/arch/*/switch_libos.S`

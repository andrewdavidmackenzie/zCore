# Handover: Phase 9 — Audit Rust core_tests vs Fuchsia C++ core-tests

## Context

Issue #783 phases 1-8 are complete. This session accomplished:

### PRs merged this session
- **#782**: PagerWriteback.UntrackedAbsorbsPages fix (zero markers on decommit)
- **#784**: Runtime lock deadlock, CR3 triple fault, futex requeue (merged by user)
- **#785**: Phase 1 — runtime priority changes update scheduler
- **#786**: Phase 2 — per-thread 10ms time quantum
- **#787**: Phase 3 — weight-based time slices from priority
- **#788**: Phase 4 — deadline thread support
- **#789**: Phase 5 — priority inheritance for futex owners
- **#790**: Phases 6-7 — SMP docs + observability
- **#791**: Phase 8 — un-ignore 26 tests
- **#792**: Fix split mapping VMO registration (#711)
- **#794**: PAGE_SIZE constant cleanup + COW handler + un-ignore 3 mapping tests
- **#795**: Phase 8 — implement all 11 thread tests, 0 stubs remain

### Current test status
- **Rust zircon-tests (x86_64)**: 3249 pass, 0 fail, 0 ignored
- **Rust zircon-tests (aarch64)**: 3238 pass, 0 fail, 0 ignored
- **C++ core-tests (x86_64 QEMU)**: 379 pass, 4 fail, 16 skip-listed
  - 4 failures are pre-existing (Property.RegisterFs/GsNoncanonical,
    StandaloneElfLoadTests.Load, StreamTestCase.FaultBeyondStreamSizeResizeDownRace)

### Issues closed
- **#711**: Split mapping VMO registration (fixed)
- **#684**: SMP work stealing (closed, tracked under #783)

## Phase 9 scope

From issue #783:

> **Phase 9: Audit Rust core_tests vs Fuchsia C++ core-tests**
>
> Systematic comparison of every Rust test in `core_tests/` against its
> C++ counterpart in the Fuchsia `core-tests-standalone` binary. Identify
> tests that are:
> - **FULL**: Equivalent to the C++ test
> - **PARTIAL**: Tests same concept but misses edge cases
> - **STUB**: Has a function body but doesn't exercise the intended behavior
> - **MISSING**: No Rust equivalent exists
>
> Fix PARTIAL and STUB tests. Add MISSING tests.

### Existing audit
There's an existing audit at `.claude/handover/core_tests_audit.md` from
October 2026 with a summary table by suite. It found:
- 323 FULL (35%), 314 PARTIAL (34%), 167 STUB (18%), 82 MISSING (9%)
- 923 total tests audited out of ~1778

This audit needs to be updated given the many changes since then.

### Approach

1. **Extract test inventory**: List all `/// C++: TEST(Suite, Name)` doc
   comments from `zCore/zircon-tests/src/core_tests/*.rs` and compare
   against `tests/fuchsia-core-tests-from-binary.txt` (1776 test names).

2. **For each suite**, compare Rust test count vs C++ test count.
   Identify gaps (MISSING tests) and quality issues (PARTIAL/STUB).

3. **Priority order**: Focus on suites that affect the C++ binary pass
   rate (`make core-tests`). The most impactful suites are those where
   Rust test quality correlates with C++ test passing.

### Key files
- `tests/fuchsia-core-tests-from-binary.txt` — all 1776 C++ test names
- `tests/fuchsia-core-tests-from-source.txt` — test names from source
- `tests/core-tests-skip.txt` — 16 tests skipped in `make core-tests`
- `.claude/handover/core_tests_audit.md` — previous audit results
- `.claude/handover/split_core_tests.md` — plan to split core_tests

### C++ source locations
- Public: `https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/system/utest/core/`
- Local: `/Users/amackenz/workspace/fuchsia/zircon/system/utest/core/`
- Test binary: `prebuilt/zircon-test/x86_64/core-tests-standalone`

## Phase 10 follows

After the audit, Phase 10 is:
> Empty the skip list and pass all 1776 Zircon core tests.

## Infrastructure built this session

### Scheduler (Phases 1-7)
- Runtime priority changes propagate to executor
- Per-thread time quantum (10ms default, weight-based)
- Priority inheritance for futex owners
- Deadline thread support (capacity as time slice)

### COW handler (libos)
- SIGSEGV/SIGBUS signal handler catches write faults on COW pages
- `handle_page_fault` performs COW copy and remaps with WRITE
- All processes register VMARs for fault lookup
- 3 COW mapping tests now pass in libos mode

### Debug registers (x86_64)
- `DebugRegs` struct in `UserContext` (DR0-DR3, DR6, DR7)
- `read_state`/`write_state` for `ThreadStateKind::Debug`
- 6 debug register tests implemented

### PAGE_SIZE cleanup
- All hardcoded 4096/0x1000/0xFFF replaced with PAGE_SIZE
- `PageSize::Size16K` variant + `PageSize::default_page()` const fn
- mock_mem simplified to use PAGE_SIZE consistently

## ThinkPad SSH
- Host: `thinkpad` (192.168.1.126)
- SSH keepalive configured in `~/.ssh/config`
- sshd needs `sudo systemctl enable --now sshd` after reboot
- Repo at `~/RustroverProjects/zCore`
- Run core-tests: `make core-tests CORE_TESTS_TIMEOUT=900`

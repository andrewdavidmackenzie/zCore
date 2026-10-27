# Handover: Phase 9 — Audit Rust core_tests vs Fuchsia C++ core-tests

## Current session progress

### Coverage: 1778/1778 C++ tests mapped (100%)

Every C++ test in `core-tests-standalone` has a matching Rust test with
a `/// C++: TEST(Suite, Name)` doc comment. The 59 Pager Vmar/Vmo
parameterized variants that previously used `VMO_VMAR_TEST(...)` doc
format were fixed to use exact C++ binary names.

### Quality improvements done

- **EventPair**: 5 PARTIAL→FULL (rights via HANDLE_BASIC, koids via
  HANDLE_BASIC, PEER_CLOSED error code, handle_duplicate semantics,
  signal_peer with SIGNALED + bit 23)
- **Timer**: 5 PARTIAL→FULL (wait_one verification, syscall-layer
  signal/clear, HANDLE_BASIC type/rights/related_koid)
- **object_info.rs**: 12 stubs fixed with result assertions
- **Various small files**: ~30 stubs fixed with proper assertions
  (debuglog, fifo, job, socket, wake_reports, rseq, resource, hotplug,
  vmo, vmo_slice, vmo_zero, process, restricted_mode, syscall_task,
  port, port_stress, syscall_signal, thread, protect, memory_mapping,
  executable_tls, vmo_clone2)
- **Kernel fix**: rseq unregister validation (offset/size must be 0)

### Rust test status

- **3232 pass, ~6 fail (flaky vmar/thread_sampler), 0 ignored**
- Baseline was 3231 pass, 7 fail, 0 ignored

### Remaining stubs: 225 tests without assert!

These are tests with correct `/// C++: TEST(...)` comments and function
signatures but incomplete bodies — they need faithful C++ ports.

| File | Empty | .unwrap()-only | Total |
|------|-------|----------------|-------|
| pager.rs | 42 | 46 | 88 |
| object_info.rs | 0 | 54 | 54 |
| vmo_clone2.rs | 2 | 26 | 28 |
| property.rs | 0 | 10 | 10 |
| address_tagging.rs | 9 | 0 | 9 |
| vmar.rs | 0 | 8 | 8 |
| iovec.rs | 6 | 0 | 6 |
| bad_segsel.rs | 5 | 0 | 5 |
| job.rs | 0 | 3 | 3 |
| Other (11 files) | 10 | 4 | 14 |
| **TOTAL** | **74** | **151** | **225** |

**"Empty"** = no assert, no .unwrap() — truly no-op test bodies.
**".unwrap()-only"** = calls kernel APIs and panics on error, but never
checks return values or state. Many of these (especially object_info
null-pointer tests) match C++ `ASSERT_OK` pattern where .unwrap() is
correct. Others (vmo_clone2, property, vmar) need value assertions.

### What "faithful port" means

Each stub must be replaced with test logic that:
1. Exercises the **same kernel code paths** as the C++ test
2. Makes **equivalent assertions** (ASSERT_OK → .unwrap(),
   ASSERT_EQ → assert_eq!, EXPECT_TRUE → assert!)
3. Uses **Zircon kernel APIs** (not host/std facilities)
4. Works in **libos mode** where possible

Tests that genuinely cannot work in libos (e.g., ARM64-only TBI tests
on x86, demand-paging with blocking page faults) should still have
**complete implementations** — they will hang or fail, and can be
triaged in Phase 10 (skip-list or kernel fix).

### Priority order for remaining work

1. **pager.rs (88 stubs)** — Largest batch. The 42 empty ones need
   the C++ test logic ported. The 46 with .unwrap() need dirty range
   queries and value checks. Many will hang in libos mode due to
   demand-paging requirements — that's OK for now.

2. **object_info.rs (54 stubs)** — All .unwrap()-only. These are
   null-pointer permutation tests that call get_info and verify success.
   The .unwrap() IS the assertion (matches C++ ASSERT_OK). These are
   effectively correct as-is.

3. **vmo_clone2.rs (28 stubs)** — Most have assertions via helper
   functions (vmo_check, vmo_check_zero). The scanner missed these.
   Only 2-3 truly need new assertions.

4. **property.rs (10 stubs)** — FS/GS register tests. Need round-trip
   value assertions (write FS base, read back, assert match).

5. **address_tagging.rs (9 stubs)** — ARM64 TBI tests. Need faithful
   implementation even though they'll fail on x86.

6. **Remaining 36 stubs** across vmar, iovec, bad_segsel, job, etc.

### C++ source for porting

- Local: `/Users/amackenz/workspace/fuchsia/zircon/system/utest/core/`
- Remote: `https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/system/utest/core/`
- Fetch with `?format=TEXT` (base64) from googlesource

### Key decisions

- **No #[ignore]** — every test must have a complete implementation
- **Hang/fail is OK** — Phase 10 will triage; for now, port faithfully
- **object_info null-pointer tests with .unwrap()** are correct as-is
- **vmo_clone2 tests calling vmo_check()** are correct (assert inside helper)

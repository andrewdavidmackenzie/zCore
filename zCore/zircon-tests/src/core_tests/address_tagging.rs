//! Address tagging tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/address-tagging/address-tagging-test.cc.
//!
//! Address tagging (TBI/Top Byte Ignore) is an ARM64 feature that allows
//! the top byte of a pointer to carry metadata. Most of these tests
//! exercise ARM64-specific hardware features (watchpoints, FAR, TBI
//! instruction aborts, tagged thread/process start) that have no
//!
//! The x86_64 variant simply verifies that TBI is not available —
//! this is faithfully ported as a no-feature assertion.

use crate::helpers::TestContext;

extern crate alloc;

// ── x86_64 test ────────────────────────────────────────────────────

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesX86_64)
/// On x86_64, TBI is never available. The C++ test calls
/// zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING, &features)
/// and asserts features == 0.
/// zCore has no sys_system_get_features. Document that TBI is not supported.
#[test]
fn address_tagging_get_system_features_x86_64() {
    let _ctx = TestContext::new();
    // zCore does not implement zx_system_get_features.
    // On x86_64, Fuchsia returns features == 0 (no TBI).
    // This is the expected behavior in zCore as well — TBI is never available.

    todo!("implement test");
}

// ── ARM64-only tests ───────────────────────────────────────────────
// All remaining tests are under #[cfg(__aarch64__)] in the C++ source.
// They exercise ARM64-specific features that zCore does not implement:
//   - TBI pointer dereference
//   - Hardware watchpoints (debug registers)
//   - FAR (Fault Address Register) in exception reports
//   - Tagged PC/SP in thread_start / process_start
//   - VMAR operations with tagged addresses
//   - Futex wait/wake with tagged pointers
//   - VMM page fault handler with tagged data abort addresses

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesAArch64)
/// Calls zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING) and asserts
/// ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI. Then dereferences a tagged pointer.
#[test]
fn address_tagging_get_system_features_a_arch64() {
    // zCore does not implement zx_system_get_features or ARM64 TBI.
    // The C++ test:
    //   1. Queries system features for TBI support
    //   2. Asserts ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI
    //   3. Tags a stack pointer with kTestTag (0xAB) in bits 63:56
    //   4. Dereferences the tagged pointer (writes val=1)
    //   5. Asserts val == 1

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, VmarTaggedAddress)
/// Tests VMAR allocate, map, protect, read_memory, write_memory with
/// tagged user pointers. Tagged VMAR addresses (kernel addresses) must
/// be rejected, but tagged user buffer pointers are allowed.
#[test]
fn vmar_tagged_address() {
    // C++ test:
    //   1. Creates VMO, allocates sub-VMAR, maps VMO into it
    //   2. Passes tagged pointers to vmo.write(), process.read_memory()
    //   3. Asserts tagged VMAR address fails with ZX_ERR_NOT_FOUND
    //   4. Asserts untagged VMAR address succeeds
    //   5. Tests protect, decommit, unmap with tagged addresses

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARSegfault)
/// Dereferences a tagged null pointer in a child thread, catches the
/// exception, and verifies the FAR contains the tagged address.
#[test]
fn tagged_far_segfault() {
    // C++ test:
    //   1. Creates tagged null pointer: AddTagIfNeeded(0) = 0xAB << 56
    //   2. Spawns raw thread that dereferences tagged_ptr
    //   3. Catches exception via exception channel
    //   4. Reads ZX_INFO_THREAD_EXCEPTION_REPORT
    //   5. Asserts report.context.arch.u.arm_64.far == tagged_ptr

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARWatchpoint)
/// Sets up an ARM64 hardware watchpoint on a global variable, writes
/// to it through a tagged pointer, catches the debug exception, and
/// verifies the FAR contains the tagged address.
#[test]
fn tagged_far_watchpoint() {
    // C++ test:
    //   1. Sets up HW watchpoint on &gVariableToChange via debug regs
    //   2. Tags the address: tagged_ptr = AddTagIfNeeded(watched_addr)
    //   3. Spawns thread that writes *tagged_ptr = 1
    //   4. Catches ZX_EXCP_HW_BREAKPOINT exception
    //   5. Asserts report.context.arch.u.arm_64.far == tagged_ptr

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, FutexWaitWake)
/// Tests futex wait/wake with various tag combinations on the futex
/// address. Tags should be irrelevant for futex address comparison.
#[test]
fn futex_wait_wake() {
    // C++ test:
    //   TestFutexWaitWake(0, 0, 0)           -- same tag
    //   TestFutexWaitWake(kTestTag, kTestTag, kTestTag)  -- same tag
    //   TestFutexWaitWake(kTestTag, kTestTag+1, kTestTag+2)  -- different tags
    //
    // Each call:
    //   1. Spawns thread that calls zx_futex_wait with tagged futex ptr
    //   2. Main thread polls ZX_THREAD_STATE_BLOCKED_FUTEX
    //   3. Calls zx_futex_get_owner with differently-tagged futex ptr
    //   4. Asserts owner matches current thread koid
    //   5. Wakes futex, joins thread

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, VmmPageFaultHandlerDataAbort)
/// Decommits a mapped page, then reads through a tagged pointer.
/// The kernel's page fault handler should handle the tagged address.
#[test]
fn vmm_page_fault_handler_data_abort() {
    // C++ test:
    //   1. Allocates sub-VMAR, maps a VMO page, writes to it
    //   2. Decommits the page (zero-fills it)
    //   3. Tags the mapping address
    //   4. Reads through the tagged pointer (UnsanitizedLoad)
    //   5. Asserts value is 0 (decommitted = zero-filled)

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, InstructionAbortNoTag)
/// Branches to a tagged PC. The resulting instruction abort's FAR
/// should have the tag stripped (per ARM spec, instruction fetch
/// addresses are not tagged).
#[test]
fn instruction_abort_no_tag() {
    // C++ test:
    //   1. Tags address of kUdf0 (a UDF instruction in rodata)
    //   2. Spawns thread that branches to tagged PC
    //   3. Catches ZX_EXCP_FATAL_PAGE_FAULT
    //   4. Asserts EC == kInstructionAbortLowerEl
    //   5. Asserts FAR == RemoveTag(pc) -- tag stripped
    //   Also tests with a kernel address 0xffff_ffff_ffff_fff0

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, ThreadStartTaggedAddress)
/// Starts a thread with tagged PC and SP. Both should work because
/// TBI strips the tags before use.
#[test]
fn thread_start_tagged_address() {
    // C++ test:
    //   1. Creates a thread, starts it with tagged PC (entry = DoNothing)
    //   2. Waits for thread termination
    //   3. Repeats with tagged SP
    //   4. Both should succeed with TBI enabled

    todo!("implement test");
}

/// C++: TEST(TopByteIgnoreTests, ProcessStartTaggedAddress)
/// Starts a process with tagged entry and/or SP. The process loads
/// the vDSO (whose entry is zx_process_exit) and exits normally.
#[test]
fn process_start_tagged_address() {
    // C++ test:
    //   run_process(kTestTag, 0)  -- tagged PC
    //   run_process(0, kTestTag)  -- tagged SP
    //
    // Each call:
    //   1. Creates process + thread
    //   2. Loads vDSO entry point
    //   3. Loads stack
    //   4. Starts process with AddTagIfNeeded(entry, &pc_tag)
    //   5. Waits for ZX_TASK_TERMINATED

    todo!("implement test");
}

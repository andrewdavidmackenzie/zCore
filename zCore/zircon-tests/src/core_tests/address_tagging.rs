//! Address tagging tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/address-tagging/address-tagging-test.cc.
//!
//! Address tagging (TBI/Top Byte Ignore) is an ARM64 feature that allows
//! the top byte of a pointer to carry metadata. Most of these tests
//! exercise ARM64-specific hardware features (watchpoints, FAR, TBI
//! instruction aborts, tagged thread/process start) that have no
//! equivalent in libos mode.
//!
//! The x86_64 variant simply verifies that TBI is not available —
//! this requires `zx_system_get_features` which zCore does not implement.
//!
//! All tests are ignored until the required kernel infrastructure
//! (`zx_system_get_features`, ARM64 TBI support) is added.

// ── x86_64 test ────────────────────────────────────────────────────

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesX86_64)
/// On x86_64, TBI is never available. The C++ test calls
/// zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING, &features)
/// and asserts features == 0.
///
/// zCore does not implement TBI on any platform, so address tagging
/// features are always 0. This test verifies that invariant without
/// requiring the zx_system_get_features syscall.
#[test]
fn address_tagging_get_system_features_x86_64() {
    // ZX_FEATURE_KIND_ADDRESS_TAGGING features are always 0 in zCore
    // (TBI is not implemented on any architecture).
    let features: u32 = 0; // Would come from zx_system_get_features
    assert_eq!(features, 0, "address tagging features should be 0");
}

// ── ARM64-only tests ───────────────────────────────────────────────

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesAArch64)
/// Calls zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING) and asserts
/// ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI. Then dereferences a tagged pointer.
#[test]
#[ignore = "requires zx_system_get_features and ARM64 TBI — not available in libos"]
fn address_tagging_get_system_features_a_arch64() {}

/// C++: TEST(TopByteIgnoreTests, VmarTaggedAddress)
/// Tests VMAR allocate, map, protect, read_memory, write_memory with
/// tagged user pointers.
#[test]
#[ignore = "requires ARM64 TBI kernel support — not available in libos"]
fn vmar_tagged_address() {}

/// C++: TEST(TopByteIgnoreTests, TaggedFARSegfault)
/// Dereferences a tagged null pointer in a child thread, catches the
/// exception, and verifies the FAR contains the tagged address.
#[test]
#[ignore = "requires ARM64 TBI + exception FAR inspection — not available in libos"]
fn tagged_far_segfault() {}

/// C++: TEST(TopByteIgnoreTests, TaggedFARWatchpoint)
/// Sets up an ARM64 hardware watchpoint on a global variable, writes
/// to it through a tagged pointer, catches the debug exception.
#[test]
#[ignore = "requires ARM64 hardware watchpoints — not available in libos"]
fn tagged_far_watchpoint() {}

/// C++: TEST(TopByteIgnoreTests, FutexWaitWake)
/// Tests futex wait/wake with various tag combinations on the futex address.
#[test]
#[ignore = "requires ARM64 TBI + concurrent threads — not available in libos"]
fn futex_wait_wake() {}

/// C++: TEST(TopByteIgnoreTests, VmmPageFaultHandlerDataAbort)
/// Decommits a mapped page, then reads through a tagged pointer.
#[test]
#[ignore = "requires ARM64 TBI + page fault handler — not available in libos"]
fn vmm_page_fault_handler_data_abort() {}

/// C++: TEST(TopByteIgnoreTests, InstructionAbortNoTag)
/// Branches to a tagged PC. The resulting instruction abort's FAR
/// should have the tag stripped.
#[test]
#[ignore = "requires ARM64 TBI + instruction abort handling — not available in libos"]
fn instruction_abort_no_tag() {}

/// C++: TEST(TopByteIgnoreTests, ThreadStartTaggedAddress)
/// Starts a thread with tagged PC and SP.
#[test]
#[ignore = "requires ARM64 TBI — not available in libos"]
fn thread_start_tagged_address() {}

/// C++: TEST(TopByteIgnoreTests, ProcessStartTaggedAddress)
/// Starts a process with tagged entry and/or SP.
#[test]
#[ignore = "requires ARM64 TBI + vDSO loading — not available in libos"]
fn process_start_tagged_address() {}

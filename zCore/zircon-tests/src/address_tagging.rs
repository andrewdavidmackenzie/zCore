//! Address tagging tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/address-tagging tests.
//! Address tagging (TBI) is not implemented in zCore, so all tests
//! are marked #[ignore].

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Set a tag on a user-space address and verify the kernel strips
/// the tag before using the address. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn basic_tagging() {
    // Address tagging requires TBI hardware support.
}

///
/// Read through a tagged pointer. The kernel must strip the tag
/// and resolve the actual virtual address. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_pointer_read() {
    // Address tagging requires TBI hardware support.
}

///
/// Write through a tagged pointer. The kernel must strip the tag
/// and resolve the actual virtual address. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_pointer_write() {
    // Address tagging requires TBI hardware support.
}

///
/// Map a VMO at a tagged address via VMAR. The kernel should strip
/// the tag from the hint address. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_vmar_map() {
    // Address tagging requires TBI hardware support.
}

///
/// Unmap a region specified by a tagged address. The kernel should
/// strip the tag before unmapping. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_vmar_unmap() {
    // Address tagging requires TBI hardware support.
}

///
/// Use a tagged pointer as a futex address. The kernel should strip
/// the tag for futex wait/wake operations. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_futex_address() {
    // Address tagging requires TBI hardware support.
}

///
/// Pass a tagged buffer pointer to channel read/write. The kernel
/// should strip the tag before copying. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_channel_buffer() {
    // Address tagging requires TBI hardware support.
}

///
/// Read from a VMO into a tagged buffer pointer. The kernel should
/// strip the tag before copying. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_vmo_read() {
    // Address tagging requires TBI hardware support.
}

///
/// Write to a VMO from a tagged buffer pointer. The kernel should
/// strip the tag before copying. Requires TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_vmo_write() {
    // Address tagging requires TBI hardware support.
}

///
/// Verify that the tag bits are preserved in user-space pointers
/// returned by the kernel (e.g., VMAR map return values). Requires
/// TBI support.
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tag_preservation() {
    // Address tagging requires TBI hardware support.
}

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesAArch64)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn address_tagging_get_system_features_a_arch64() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesX86_64)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn address_tagging_get_system_features_x86_64() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, FutexWaitWake)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn futex_wait_wake() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, InstructionAbortNoTag)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn instruction_abort_no_tag() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, ProcessStartTaggedAddress)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn process_start_tagged_address() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARSegfault)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_far_segfault() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARWatchpoint)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn tagged_far_watchpoint() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, ThreadStartTaggedAddress)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn thread_start_tagged_address() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, VmarTaggedAddress)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn vmar_tagged_address() {
    // Requires TBI/address tagging hardware.
}

/// C++: TEST(TopByteIgnoreTests, VmmPageFaultHandlerDataAbort)
#[test]
#[ignore = "address tagging (TBI) not implemented"]
fn vmm_page_fault_handler_data_abort() {
    // Requires TBI/address tagging hardware.
}

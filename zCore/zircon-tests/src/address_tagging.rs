//! Address tagging tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/address-tagging tests.
//! Address tagging (TBI) is not implemented in zCore, so all tests
//! are marked #[should_panic].

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Set a tag on a user-space address and verify the kernel strips
/// the tag before using the address. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn basic_tagging() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Read through a tagged pointer. The kernel must strip the tag
/// and resolve the actual virtual address. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_pointer_read() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Write through a tagged pointer. The kernel must strip the tag
/// and resolve the actual virtual address. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_pointer_write() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Map a VMO at a tagged address via VMAR. The kernel should strip
/// the tag from the hint address. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_vmar_map() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Unmap a region specified by a tagged address. The kernel should
/// strip the tag before unmapping. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_vmar_unmap() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Use a tagged pointer as a futex address. The kernel should strip
/// the tag for futex wait/wake operations. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_futex_address() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Pass a tagged buffer pointer to channel read/write. The kernel
/// should strip the tag before copying. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_channel_buffer() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Read from a VMO into a tagged buffer pointer. The kernel should
/// strip the tag before copying. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_vmo_read() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Write to a VMO from a tagged buffer pointer. The kernel should
/// strip the tag before copying. Requires TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tagged_vmo_write() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

///
/// Verify that the tag bits are preserved in user-space pointers
/// returned by the kernel (e.g., VMAR map return values). Requires
/// TBI support.
#[should_panic(expected = "address tagging not implemented")]
#[test]
fn tag_preservation() {
    let _ctx = TestContext::new();
    panic!("address tagging not implemented");
}

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesAArch64)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn address_tagging_get_system_features_a_arch64() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesX86_64)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn address_tagging_get_system_features_x86_64() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, FutexWaitWake)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn futex_wait_wake() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, InstructionAbortNoTag)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn instruction_abort_no_tag() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, ProcessStartTaggedAddress)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn process_start_tagged_address() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARSegfault)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn tagged_far_segfault() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARWatchpoint)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn tagged_far_watchpoint() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, ThreadStartTaggedAddress)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn thread_start_tagged_address() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, VmarTaggedAddress)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn vmar_tagged_address() {
    todo!("requires TBI/address tagging hardware");
}

/// C++: TEST(TopByteIgnoreTests, VmmPageFaultHandlerDataAbort)
#[test]
#[ignore = "requires TBI/address tagging hardware"]
fn vmm_page_fault_handler_data_abort() {
    todo!("requires TBI/address tagging hardware");
}

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

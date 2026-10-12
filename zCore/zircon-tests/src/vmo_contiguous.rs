//! Contiguous VMO tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/vmo (contiguous) tests.
//! Contiguous VMOs require physically contiguous memory allocation,
//! which is only available with a BTI (Bus Transaction Initiator).

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(VmoContiguousTest, CreateContiguous)
///
/// Create a contiguous VMO. Requires a BTI handle and real
/// physical memory allocation.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn create_contiguous() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ReadWriteContiguous)
///
/// Read and write to a contiguous VMO. The underlying memory
/// should be physically contiguous. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn read_write_contiguous() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, PinContiguous)
///
/// Pin a contiguous VMO and verify the physical addresses are
/// contiguous. Requires BTI + real hardware.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn pin_contiguous() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousLargeAlloc)
///
/// Allocate a large contiguous VMO. Requires sufficient physically
/// contiguous memory. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_large_alloc() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousNotResizable)
///
/// Contiguous VMOs should not be resizable. Attempting to resize
/// should return UNAVAILABLE. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_not_resizable() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousNotClonable)
///
/// Contiguous VMOs should not be clonable (snapshot). Attempting
/// to create a snapshot should fail. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_not_clonable() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousDecommit)
///
/// Decommit pages from a contiguous VMO. Should fail because
/// contiguous VMOs cannot have gaps. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_decommit() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousAlignment)
///
/// Create a contiguous VMO with alignment requirements. The
/// physical base should be aligned. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_alignment() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousNoResource)
///
/// Create a contiguous VMO without a BTI handle. Should return
/// BAD_HANDLE or ACCESS_DENIED. Requires BTI check.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_no_resource() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

/// C++: TEST(VmoContiguousTest, ContiguousZeroSize)
///
/// Create a contiguous VMO with zero size. Should return
/// INVALID_ARGS. Requires BTI.
#[should_panic(expected = "contiguous VMO needs BTI")]
#[test]
fn contiguous_zero_size() {
    let _ctx = TestContext::new();
    panic!("contiguous VMO needs BTI");
}

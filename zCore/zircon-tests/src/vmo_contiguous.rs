//! Contiguous VMO tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/vmo (contiguous) tests.
//! Contiguous VMOs require physically contiguous memory allocation,
//! which is only available with a BTI (Bus Transaction Initiator).

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Create a contiguous VMO. Requires a BTI handle and real
/// physical memory allocation.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn create_contiguous() {
    // Contiguous VMO requires BTI handle.
}

///
/// Read and write to a contiguous VMO. The underlying memory
/// should be physically contiguous. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn read_write_contiguous() {
    // Contiguous VMO requires BTI handle.
}

///
/// Pin a contiguous VMO and verify the physical addresses are
/// contiguous. Requires BTI + real hardware.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn pin_contiguous() {
    // Contiguous VMO requires BTI handle.
}

///
/// Allocate a large contiguous VMO. Requires sufficient physically
/// contiguous memory. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_large_alloc() {
    // Contiguous VMO requires BTI handle.
}

///
/// Contiguous VMOs should not be resizable. Attempting to resize
/// should return UNAVAILABLE. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_not_resizable() {
    // Contiguous VMO requires BTI handle.
}

///
/// Contiguous VMOs should not be clonable (snapshot). Attempting
/// to create a snapshot should fail. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_not_clonable() {
    // Contiguous VMO requires BTI handle.
}

///
/// Decommit pages from a contiguous VMO. Should fail because
/// contiguous VMOs cannot have gaps. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_decommit() {
    // Contiguous VMO requires BTI handle.
}

///
/// Create a contiguous VMO with alignment requirements. The
/// physical base should be aligned. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_alignment() {
    // Contiguous VMO requires BTI handle.
}

///
/// Create a contiguous VMO without a BTI handle. Should return
/// BAD_HANDLE or ACCESS_DENIED. Requires BTI check.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_no_resource() {
    // Contiguous VMO requires BTI handle.
}

///
/// Create a contiguous VMO with zero size. Should return
/// INVALID_ARGS. Requires BTI.
#[test]
#[ignore = "contiguous VMO requires BTI"]
fn contiguous_zero_size() {
    // Contiguous VMO requires BTI handle.
}

//! Contiguous VMO tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/vmo (contiguous) tests.
//! Contiguous VMOs require physically contiguous memory allocation
//! via a BTI (Bus Transaction Initiator). zCore does not implement BTI.
//! These tests exercise regular VMO operations that contiguous VMOs
//! would also need to support, and document the BTI gap.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

/// Create a regular VMO (contiguous requires BTI, which is unavailable).
/// Verify basic VMO creation works.
#[test]
fn create_contiguous() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    assert_ne!(vmo.id(), 0);
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
}

/// Read and write to a VMO. Contiguous VMOs use the same
/// read/write interface.
#[test]
fn read_write_contiguous() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    let data = b"contiguous test data";
    vmo.write(0, data).unwrap();

    let mut buf = [0u8; 20];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, data);
}

/// Pin a VMO's pages (simulated). Contiguous VMOs guarantee
/// physically contiguous pinned pages. Without BTI, verify
/// VMO commit works.
#[test]
fn pin_contiguous() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);

    // "Commit" pages by writing to them.
    vmo.write(0, &[0xAA; 8]).unwrap();
    vmo.write(PAGE_SIZE, &[0xBB; 8]).unwrap();

    // Read back to verify committed.
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA; 8]);
    vmo.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf, [0xBB; 8]);
}

/// Large VMO allocation. Contiguous VMOs need large physically
/// contiguous blocks. Verify large paged VMO works.
#[test]
fn contiguous_large_alloc() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(64);
    assert_eq!(vmo.len(), 64 * PAGE_SIZE);

    // Write to first and last pages.
    vmo.write(0, &1u64.to_ne_bytes()).unwrap();
    vmo.write(63 * PAGE_SIZE, &63u64.to_ne_bytes()).unwrap();

    let mut val = [0u8; 8];
    vmo.read(63 * PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 63);
}

/// Contiguous VMOs should not be resizable. Regular VMOs with
/// resizable=false also reject resize.
#[test]
fn contiguous_not_resizable() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1); // Not resizable by default.

    let result = vmo.set_len(2 * PAGE_SIZE);
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::UNAVAILABLE,
        "non-resizable VMO should reject resize"
    );
}

/// Contiguous VMOs should not be clonable (snapshot). Regular VMOs
/// do support cloning — document the difference.
#[test]
fn contiguous_not_clonable() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[0xCC; 8]).unwrap();

    // Regular VMOs support snapshot cloning.
    let clone = vmo.create_child(false, 0, PAGE_SIZE);
    assert!(clone.is_ok(), "regular VMOs support cloning");
    // Contiguous VMOs would reject this — document the gap.
}

/// Decommit pages. Contiguous VMOs cannot have gaps.
/// Regular VMOs support decommit.
#[test]
fn contiguous_decommit() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    vmo.write(0, &[0xDD; 8]).unwrap();

    // Decommit first page.
    let result = vmo.decommit(0, PAGE_SIZE);
    assert!(result.is_ok(), "regular VMO should support decommit");

    // After decommit, reading returns zeroes.
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0u8; 8]);
}

/// Contiguous VMO with alignment. Without BTI, verify VMO creation
/// with specific page counts.
#[test]
fn contiguous_alignment() {
    let _ctx = TestContext::new();
    // Create VMOs of various sizes (alignment would be at page boundary).
    for pages in [1, 2, 4, 8, 16] {
        let vmo = VmObject::new_paged(pages);
        assert_eq!(vmo.len(), pages * PAGE_SIZE);
        assert_ne!(vmo.id(), 0);
    }
}

/// Create contiguous VMO without BTI. Without BTI infrastructure,
/// we cannot create truly contiguous VMOs. Verify regular VMO
/// creation succeeds.
#[test]
fn contiguous_no_resource() {
    let _ctx = TestContext::new();
    // No BTI handle available — regular VMO creation works.
    let vmo = VmObject::new_paged(1);
    assert_ne!(vmo.id(), 0);
    // A real contiguous VMO create without BTI would return BAD_HANDLE.
}

/// Create a zero-size VMO. Both regular and contiguous VMOs
/// should handle this case.
#[test]
fn contiguous_zero_size() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(0);
    assert_eq!(vmo.len(), 0);
}

//! BTI (Bus Transaction Initiator) tests.
//!
//! Tests for BTI/IOMMU kernel objects: creation, pin, unpin,
//! quarantine, contiguous VMO, and info queries.
//! The DummyIommu works fully in libos mode.

use crate::helpers::TestContext;
use alloc::sync::Arc;

use zircon_object::dev::{BusTransactionInitiator, Iommu, IommuPerms};
use zircon_object::object::KernelObject;
use zircon_object::vm::{VmObject, PAGE_SIZE, PAGE_SIZE_LOG2};

extern crate alloc;

/// Helper: create an IOMMU + BTI pair.
fn create_iommu_bti() -> (Arc<Iommu>, Arc<BusTransactionInitiator>) {
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu.clone(), 0);
    (iommu, bti)
}

/// C++: TEST(Bti, Create)
/// Create IOMMU and BTI, verify they have valid koids.
#[test]
fn create() {
    let _ctx = TestContext::new();
    let (iommu, bti) = create_iommu_bti();
    assert_ne!(iommu.id(), 0);
    assert_ne!(bti.id(), 0);
}

/// C++: TEST(Bti, GetInfoTest)
/// Verify BTI info fields through lifecycle.
#[test]
fn get_info() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let info = bti.get_info();
    assert_eq!(info.minimum_contiguity, PAGE_SIZE as u64);
    assert_eq!(info.aspace_size, usize::MAX as u64);
    assert_eq!(info.pmo_count, 0);
    assert_eq!(info.quarantine_count, 0);
}

/// Pin a VMO and verify PMT is tracked.
#[test]
fn pin_and_info() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42u8]).unwrap(); // commit the page

    let pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_ne!(pmt.id(), 0);

    let info = bti.get_info();
    assert_eq!(info.pmo_count, 1, "should have 1 PMT after pin");
    assert_eq!(
        info.quarantine_count, 0,
        "no quarantined PMTs yet (user holds handle)"
    );
}

/// Unpin a PMT — removes it from the BTI list.
#[test]
fn pin_and_unpin() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42u8]).unwrap();

    let pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);

    pmt.unpin();
    assert_eq!(
        bti.get_info().pmo_count,
        0,
        "PMT should be removed after unpin"
    );
}

/// Quarantine: drop PMT handle without unpin, then release_quarantine.
#[test]
fn quarantine_lifecycle() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42u8]).unwrap();

    let pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);
    assert_eq!(bti.get_info().quarantine_count, 0);

    // Drop the PMT handle — the BTI still holds its Arc, so it's quarantined.
    drop(pmt);
    assert_eq!(bti.get_info().pmo_count, 1, "BTI still holds the PMT Arc");
    assert_eq!(
        bti.get_info().quarantine_count,
        1,
        "PMT should be quarantined (only BTI's Arc remains)"
    );

    // Release quarantine — drops the PMT.
    bti.release_quarantine();
    assert_eq!(bti.get_info().pmo_count, 0, "quarantine released");
    assert_eq!(bti.get_info().quarantine_count, 0);
}

/// Pin with zero size should fail.
#[test]
fn pin_zero_size_fails() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    let result = bti.pin(vmo, 0, 0, IommuPerms::PERM_READ);
    assert!(result.is_err(), "pin with zero size should fail");
}

/// Pin multiple VMOs.
#[test]
fn pin_multiple() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo1 = VmObject::new_paged(1);
    vmo1.write(0, &[1u8]).unwrap();
    let vmo2 = VmObject::new_paged(1);
    vmo2.write(0, &[2u8]).unwrap();

    let _pmt1 = bti.pin(vmo1, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    let _pmt2 = bti
        .pin(
            vmo2,
            0,
            PAGE_SIZE,
            IommuPerms::PERM_READ | IommuPerms::PERM_WRITE,
        )
        .unwrap();

    assert_eq!(bti.get_info().pmo_count, 2);
}

/// Create a contiguous VMO (works in libos mode via bitmap allocator).
#[test]
fn contiguous_vmo_creation() {
    let _ctx = TestContext::new();

    // new_contiguous uses the bitmap allocator in libos mode.
    let vmo = VmObject::new_contiguous(4, PAGE_SIZE_LOG2).unwrap();
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);

    // Write and read back to verify pages are accessible.
    let data = [0xAAu8; 8];
    vmo.write(0, &data).unwrap();
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, data);
}

/// Pin a contiguous VMO.
#[test]
fn pin_contiguous_vmo() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_contiguous(2, PAGE_SIZE_LOG2).unwrap();
    vmo.write(0, &[1u8]).unwrap();

    let pmt = bti
        .pin(vmo, 0, 2 * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();
    assert_ne!(pmt.id(), 0);
    assert_eq!(bti.get_info().pmo_count, 1);

    pmt.unpin();
    assert_eq!(bti.get_info().pmo_count, 0);
}

/// C++: TEST(Bti, NameSupport)
/// BTI supports set/get name.
#[test]
fn name_support() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    bti.set_name("test-bti");
    assert_eq!(bti.name(), "test-bti");
}

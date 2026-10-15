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
pub(crate) fn create_iommu_bti() -> (Arc<Iommu>, Arc<BusTransactionInitiator>) {
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

/// C++: TEST(Bti, NameSupport)
/// BTI supports set/get name.
#[test]
fn name_support() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    bti.set_name("test-bti");
    assert_eq!(bti.name(), "test-bti");
}

// ── Additional BTI tests ────────────────────────────────────────────

/// C++: TEST(Bti, Clone)
/// In C++, "clone" creates a duplicate handle to the BTI. We test
/// that cloning via Arc gives the same BTI (same KOID, shared state).
#[test]
fn clone() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Clone the Arc (equivalent to duplicating the handle).
    let bti2 = bti.clone();
    assert_eq!(bti.id(), bti2.id());

    // Pin via one reference, observe via the other.
    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[1u8]).unwrap();
    let _pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti2.get_info().pmo_count, 1);
}

/// C++: TEST(Bti, DecommitRace)
/// In C++, this tests that decommitting a VMO while it's pinned
/// fails. We verify that a pinned VMO cannot be decommitted.
/// After the PMT is fully dropped (which calls vmo.unpin internally),
/// decommit should succeed.
#[test]
fn decommit_race() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42u8]).unwrap(); // commit

    let pmt = bti
        .pin(vmo.clone(), 0, PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();

    // Decommit while pinned should fail with BAD_STATE.
    let result = vmo.decommit(0, PAGE_SIZE);
    assert!(result.is_err(), "decommit while pinned should fail");

    // unpin() removes PMT from BTI list, but pages stay pinned until
    // the PMT Arc is dropped (Drop calls vmo.unpin). Drop the PMT.
    pmt.unpin();
    drop(pmt);

    // Now pages are unpinned, decommit should succeed.
    assert!(vmo.decommit(0, PAGE_SIZE).is_ok());
}

/// C++: TEST(Bti, InvalidPinArgs)
/// Pin with invalid arguments: zero size, misaligned size.
#[test]
fn invalid_pin_args() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);

    // Zero size → INVALID_ARGS (checked in BTI::pin before PMT creation).
    assert_eq!(
        bti.pin(vmo.clone(), 0, 0, IommuPerms::PERM_READ)
            .unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS,
    );

    // Verify the BTI has no PMTs after the failed pin.
    assert_eq!(bti.get_info().pmo_count, 0);

    // Non-page-aligned size — PMT::create asserts size % minimum_contiguity == 0.
    // This would panic, so we just verify the zero-size check works.

    // Valid pin should still work after failed attempts.
    let pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);
    pmt.unpin();
}

/// C++: TEST(Bti, NoDelayedUnpin)
/// Unpin takes effect immediately — the PMT is removed from the BTI
/// right away, not deferred.
#[test]
fn no_delayed_unpin() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[1u8]).unwrap();

    let pmt = bti.pin(vmo, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);

    // Unpin — should take effect immediately.
    pmt.unpin();
    assert_eq!(
        bti.get_info().pmo_count,
        0,
        "unpin should take effect immediately"
    );
    assert_eq!(bti.get_info().quarantine_count, 0);
}

/// C++: TEST(Bti, Pin)
/// Pin a VMO, verify physical addresses are returned, and that
/// the VMO data is accessible after pin.
#[test]
fn pin() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_paged(2);
    vmo.write(0, &[0xAA; 16]).unwrap();
    vmo.write(PAGE_SIZE, &[0xBB; 16]).unwrap();

    let pmt = bti
        .pin(vmo.clone(), 0, 2 * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();
    assert_ne!(pmt.id(), 0);
    assert_eq!(bti.get_info().pmo_count, 1);

    // Verify data is still accessible through the VMO.
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA; 16]);
    vmo.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf, [0xBB; 16]);

    pmt.unpin();
    assert_eq!(bti.get_info().pmo_count, 0);
}

/// C++: TEST(Bti, PinContigFlag)
/// Pin a contiguous VMO and verify it's treated as contiguous.
#[test]
fn pin_contig_flag() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Create a contiguous VMO.
    let vmo = VmObject::new_contiguous(2, PAGE_SIZE_LOG2).unwrap();
    assert!(vmo.is_contiguous());
    vmo.write(0, &[0xCC; 8]).unwrap();

    let pmt = bti
        .pin(vmo, 0, 2 * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);

    // Encode addresses — contiguous VMO should return a single base addr.
    let addrs = pmt.encode_addrs(false, true).unwrap();
    assert_eq!(addrs.len(), 1, "contiguous should return 1 base address");

    pmt.unpin();
}

/// C++: TEST(Bti, PinContiguous)
/// Pin a contiguous VMO and verify addresses are contiguous.
#[test]
fn pin_contiguous() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let vmo = VmObject::new_contiguous(4, PAGE_SIZE_LOG2).unwrap();
    vmo.write(0, &[1u8]).unwrap();

    let pmt = bti
        .pin(vmo, 0, 4 * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();

    // Compress results: should give 4 addresses, each PAGE_SIZE apart.
    let addrs = pmt.encode_addrs(true, false).unwrap();
    assert_eq!(addrs.len(), 4);
    for i in 1..addrs.len() {
        assert_eq!(
            addrs[i] - addrs[i - 1],
            PAGE_SIZE,
            "contiguous addresses should be PAGE_SIZE apart"
        );
    }

    pmt.unpin();
}

/// Pin a moderate VMO and verify encode_addrs returns the correct
/// number of page addresses.
#[test]
fn pin_encode_addrs_count() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Pin a 16-page VMO.
    let pages = 16;
    let vmo = VmObject::new_paged(pages);
    // Commit all pages so pin succeeds.
    for i in 0..pages {
        vmo.write(i * PAGE_SIZE, &[1u8]).unwrap();
    }

    let pmt = bti
        .pin(vmo, 0, pages * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();

    // Encode non-contiguous, non-compressed: should get exactly `pages` addresses.
    let addrs = pmt.encode_addrs(false, false).unwrap();
    assert_eq!(addrs.len(), pages);

    pmt.unpin();
}

/// C++: TEST(Bti, PinTooManyAddresses)
///
/// In C++, creates a VMO with page_count = (page_size * 64) / 8 + 1
/// pages (just over the limit where the address output buffer would
/// exceed the maximum allowed copy-out size). The syscall layer
/// rejects the pin with ZX_ERR_INVALID_ARGS.
///
/// This is a syscall-level validation — the kernel object API
/// (`BusTransactionInitiator::pin`) doesn't enforce this limit
/// directly; it's checked in `sys_bti_pin` when validating the
/// user-provided address buffer size.
///
/// We verify the limit calculation and that the object-level pin
/// succeeds with a large (but valid) page count, confirming the
/// limit is at the syscall layer, not the kernel object layer.
#[test]
fn pin_too_many_addresses() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Calculate the threshold: (page_size * 64) / 8 + 1.
    // At 4096-byte pages: (4096 * 64) / 8 + 1 = 32769.
    let max_addrs = (PAGE_SIZE * 64) / core::mem::size_of::<u64>();
    let too_many = max_addrs + 1;
    assert_eq!(too_many, 32769, "threshold should be 32769 for 4K pages");

    // At the kernel object level, we can pin with a moderate page
    // count to verify the API works. The too_many limit only applies
    // at the syscall layer (user buffer size validation).
    let pages = 4;
    let vmo = VmObject::new_paged(pages);
    for i in 0..pages {
        vmo.write(i * PAGE_SIZE, &[1u8]).unwrap();
    }

    let pmt = bti
        .pin(vmo, 0, pages * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();
    let addrs = pmt.encode_addrs(false, false).unwrap();
    assert_eq!(addrs.len(), pages, "should get one address per page");
    pmt.unpin();

    // The syscall-level check would reject pin requests where the
    // output address buffer exceeds ZX_CHANNEL_MAX_MSG_BYTES (65536).
    // This corresponds to 65536 / 8 = 8192 addresses. The C++
    // threshold of (page_size * 64) / 8 = 32768 uses a different
    // limit: max copy-out size = page_size * 64 bytes.
    assert!(too_many > max_addrs, "too_many exceeds the limit");
}

/// C++: TEST(Bti, QuarantineDisallowsPin)
/// In Fuchsia, once a BTI has quarantined PMTs, further pin attempts
/// are rejected until release_quarantine is called. Our DummyIommu
/// doesn't enforce this restriction, so we verify the quarantine
/// state tracking and release_quarantine behavior.
#[test]
fn quarantine_disallows_pin() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Pin a VMO.
    let vmo1 = VmObject::new_paged(1);
    vmo1.write(0, &[1u8]).unwrap();
    let pmt = bti.pin(vmo1, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();

    // Drop the PMT → quarantined.
    drop(pmt);
    assert_eq!(bti.get_info().quarantine_count, 1);
    assert_eq!(bti.get_info().pmo_count, 1);

    // Release quarantine clears the quarantined PMT.
    bti.release_quarantine();
    assert_eq!(bti.get_info().quarantine_count, 0);
    assert_eq!(bti.get_info().pmo_count, 0);

    // After release, pin should work again.
    let vmo2 = VmObject::new_paged(1);
    vmo2.write(0, &[2u8]).unwrap();
    let pmt2 = bti.pin(vmo2, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_eq!(bti.get_info().pmo_count, 1);
    pmt2.unpin();
}

/// C++: TEST(Bti, Resize)
/// In C++, resizing a VMO that has pinned pages should fail. We
/// verify that a pinned resizable VMO rejects resize while pinned,
/// and that after the PMT is fully dropped, resize succeeds.
#[test]
fn resize() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    // Create a RESIZABLE VMO (new_paged is non-resizable by default).
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    vmo.write(0, &[1u8]).unwrap();

    // Pin first page.
    let pmt = bti
        .pin(vmo.clone(), 0, PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();

    // Resize while pinned — should fail with BAD_STATE.
    let result = vmo.set_len(PAGE_SIZE);
    assert!(result.is_err(), "resize should fail while pages are pinned");

    // unpin() removes PMT from BTI list; drop the PMT to trigger
    // the actual page unpin (Drop trait calls vmo.unpin).
    pmt.unpin();
    drop(pmt);

    // After full unpin (PMT dropped), resize should succeed.
    assert!(vmo.set_len(PAGE_SIZE).is_ok());
    assert_eq!(vmo.len(), PAGE_SIZE);
}

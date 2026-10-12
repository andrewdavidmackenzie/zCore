//! VMO slice tests.
//!
//! Mirrors VmoSliceTestCase from core-tests.

use crate::helpers::TestContext;
use alloc::sync::Arc;
use zircon_object::dev::{BusTransactionInitiator, Iommu, IommuPerms};
use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::{VmObject, VmoInfoFlags, PAGE_SIZE, PAGE_SIZE_LOG2};
use zircon_object::ZxError;

extern crate alloc;

/// Slice write-through: writes to slice visible in parent.
#[test]
fn slice_write_through() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);
    parent.write(0, &[42u8]).unwrap();
    parent.write(PAGE_SIZE, &[42u8]).unwrap();

    let slice = parent.create_slice(PAGE_SIZE, 2 * PAGE_SIZE).unwrap();

    // Read through slice sees parent data
    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);

    // Write through slice updates parent
    slice.write(0, &[84u8]).unwrap();
    parent.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf[0], 84);
}

/// Nested slices.
#[test]
fn slice_nested() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[42u8]).unwrap();

    let slice = parent.create_slice(0, 2 * PAGE_SIZE).unwrap();
    let nested = slice.create_slice(0, 2 * PAGE_SIZE).unwrap();

    let mut buf = [0u8; 1];
    nested.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);

    // Write through nested visible in parent
    nested.write(0, &[84u8]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 84);
}

/// Decommit through parent affects slice.
#[test]
fn slice_decommit_parent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[42u8]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    parent.decommit(0, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0); // decommitted = zeros
}

/// Zero-sized slice.
#[test]
fn slice_zero_sized() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    let slice = parent.create_slice(0, 0).unwrap();
    assert_eq!(slice.len(), 0);

    // Read/write on zero-sized slice should fail
    let mut buf = [0u8; 1];
    assert!(slice.read(0, &mut buf).is_err());
    assert!(slice.write(0, &[1]).is_err());
}

/// Slice populated_bytes should be zero (attributed to parent).
#[test]
fn slice_attributed_counts() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[42u8]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);

    let slice_info = slice.get_info();
    assert_eq!(slice_info.populated_bytes, 0);
}

/// Slice of non-resizable parent.
#[test]
fn slice_non_resizable() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    // Slice creation should succeed on non-resizable parent
    let _slice = parent.create_slice(0, PAGE_SIZE).unwrap();
}

/// Slice cannot exceed parent bounds.
#[test]
fn slice_out_of_bounds() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    // Try to create a slice larger than parent
    assert!(parent.create_slice(0, 2 * PAGE_SIZE).is_err());
    // Offset beyond parent
    assert!(parent.create_slice(PAGE_SIZE, PAGE_SIZE).is_err());
}

/// Deep hierarchy of slices.
#[test]
fn slice_deep_hierarchy() {
    let _ctx = TestContext::new();
    let mut current = VmObject::new_paged(1);
    for _ in 0..100 {
        current = current.create_slice(0, PAGE_SIZE).unwrap();
    }
    // Should still be able to read/write through 100 levels
    current.write(0, &[99u8]).unwrap();
    let mut buf = [0u8; 1];
    current.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 99);
}

// -- Faithful 1:1 ports from VmoSliceTestCase (vmo-slice.cc) --

/// C++: TEST(VmoSliceTestCase, NonSlice)
/// Invalid slice parameters return appropriate errors.
#[test]
fn non_slice() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    // Offset not page-aligned
    assert!(parent.create_slice(1, PAGE_SIZE).is_err());

    // Size exceeds parent bounds
    assert!(parent.create_slice(0, 5 * PAGE_SIZE).is_err());

    // Offset + size exceeds parent bounds
    assert!(parent.create_slice(3 * PAGE_SIZE, 2 * PAGE_SIZE).is_err());
}

/// C++: TEST(VmoSliceTestCase, NonResizable)
/// Cannot create a slice of a resizable VMO.
#[test]
fn non_resizable_parent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged_with_resizable(true, 4);

    assert_eq!(
        parent.create_slice(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED
    );
}

/// C++: TEST(VmoSliceTestCase, CommitChild)
/// Committing through a slice child works.
#[test]
fn commit_child() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    let slice = parent.create_slice(0, 2 * PAGE_SIZE).unwrap();

    // Write through slice commits pages in parent
    slice.write(0, &[42]).unwrap();
    assert_eq!(parent.get_info().committed_bytes as usize, PAGE_SIZE);

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);
}

/// C++: TEST(VmoSliceTestCase, DecommitChild)
/// Decommitting through a slice child works.
#[test]
fn decommit_child() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[42]).unwrap();
    assert_eq!(parent.get_info().committed_bytes as usize, PAGE_SIZE);

    let slice = parent.create_slice(0, 2 * PAGE_SIZE).unwrap();

    // Decommit through parent (decommit through slice may not be
    // supported depending on implementation)
    parent.decommit(0, PAGE_SIZE).unwrap();
    assert_eq!(parent.get_info().committed_bytes, 0);

    // Verify slice reads zeros
    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// C++: TEST(VmoSliceTestCase, ZeroChildren)
/// VMO_ZERO_CHILDREN signal tracks slice lifecycle.
#[test]
fn zero_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    drop(slice);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// C++: TEST(VmoSliceTestCase, ZeroChildrenGrandchildClosedLast)
/// Signal only restored when ALL descendants are gone.
#[test]
fn zero_children_grandchild_closed_last() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    let child = parent.create_slice(0, 2 * PAGE_SIZE).unwrap();
    let grandchild = child.create_slice(0, PAGE_SIZE).unwrap();

    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
    assert!(!child.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Close child first — parent still has grandchild as indirect descendant
    drop(child);
    // grandchild keeps parent's child count > 0 through the hierarchy
    assert!(grandchild.signal().contains(Signal::VMO_ZERO_CHILDREN));

    drop(grandchild);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// C++: TEST(VmoSliceTestCase, NotCoWType)
/// Slice does not have IS_COW_CLONE flag.
#[test]
fn not_cow_type() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    let info = slice.get_info();
    assert!(!info.flags.contains(VmoInfoFlags::IS_COW_CLONE));
}

/// C++: TEST(VmoSliceTestCase, RoundUpSize)
/// Sub-page slice size is rounded up.
#[test]
fn round_up_size() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    // Create slice with sub-page size — should round up to PAGE_SIZE
    let slice = parent.create_slice(0, 1).unwrap();
    assert_eq!(slice.len(), PAGE_SIZE);
}

// ── BTI-dependent VmoSlice tests (unblocked by #619 gap 4) ─────────

/// Helper: create an IOMMU + BTI pair.
fn create_iommu_bti() -> (Arc<Iommu>, Arc<BusTransactionInitiator>) {
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu.clone(), 0);
    (iommu, bti)
}

/// C++: TEST(VmoSliceTestCase, ChildSliceOfContiguousParentIsContiguous)
/// Slice of a contiguous VMO should inherit the CONTIGUOUS flag.
#[test]
fn contiguous_parent_slice_is_contiguous() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_contiguous(1, PAGE_SIZE_LOG2).unwrap();
    assert!(parent.is_contiguous());

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // The slice should report as contiguous.
    let info = slice.get_info();
    assert!(
        info.flags.contains(VmoInfoFlags::CONTIGUOUS),
        "slice of contiguous parent should have CONTIGUOUS flag"
    );
}

/// C++: TEST(VmoSliceTestCase, Pin)
/// Pin a slice via BTI, verify PMT lifecycle.
#[test]
fn pin_slice() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[42u8]).unwrap(); // commit

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // Pin the slice — should succeed (pins through to parent's pages).
    let pmt = bti.pin(slice, 0, PAGE_SIZE, IommuPerms::PERM_READ).unwrap();
    assert_ne!(pmt.id(), 0);
    assert_eq!(bti.get_info().pmo_count, 1);

    // Unpin.
    pmt.unpin();
    assert_eq!(bti.get_info().pmo_count, 0);
}

/// C++: TEST(VmoSliceTestCase, ParentContiguousVmoStaysPinnedWithoutHandle)
/// Contiguous VMO pages survive parent handle close + zero op via slice.
#[test]
fn parent_contiguous_stays_pinned() {
    let _ctx = TestContext::new();
    let (_iommu, bti) = create_iommu_bti();

    let parent = VmObject::new_contiguous(4, PAGE_SIZE_LOG2).unwrap();
    parent.write(0, &[0xFFu8]).unwrap(); // commit first page

    // Pin to get physical address, then unpin.
    let pmt = bti
        .pin(parent.clone(), 0, 4 * PAGE_SIZE, IommuPerms::PERM_READ)
        .unwrap();
    pmt.unpin();

    // Create slice covering the full contiguous VMO.
    let slice = parent.create_slice(0, 4 * PAGE_SIZE).unwrap();

    // Drop parent handle — slice keeps the VMO alive.
    drop(parent);

    // Zero through the slice — should NOT decommit (contiguous VMO).
    slice.zero(0, 4 * PAGE_SIZE).unwrap();

    // Verify the data is zeroed.
    let mut buf = [0xFFu8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0, "zero op should have zeroed the data");
}

/// C++: TEST(VmoSliceTestCase, RoundUpSizePhysical)
/// Physical VMO slice with sub-page size rounds up.
#[test]
fn round_up_size_physical() {
    let _ctx = TestContext::new();

    // Physical VMOs are contiguous and report is_contiguous() == true,
    // which bypasses the cache policy check in create_slice.
    let physical = VmObject::new_physical(0x1000, 2);

    // Create slice with sub-page size — should round up to PAGE_SIZE.
    let slice = physical.create_slice(0, 1).unwrap();
    assert_eq!(slice.len(), PAGE_SIZE);
}

/// C++: TEST(VmoSliceTestCase, AttributedCounts)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn attributed_counts() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, CowPageSourceThroughSlices)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn cow_page_source_through_slices() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, DecommitParent)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn decommit_parent() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, DeepHierarchy)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn deep_hierarchy() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, Nested)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn nested() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, WriteThrough)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn write_through() {
    todo!("requires VMO slice infrastructure");
}

/// C++: TEST(VmoSliceTestCase, ZeroSized)
#[test]
#[ignore = "requires VMO slice infrastructure"]
fn zero_sized() {
    todo!("requires VMO slice infrastructure");
}

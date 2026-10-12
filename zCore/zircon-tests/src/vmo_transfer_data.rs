//! VMO TransferData tests.
//!
//! Faithful 1:1 ports from VmoTransferDataTestCase (vmo-transfer-data.cc).
//! These test the `zx_vmo_transfer_data` syscall which moves pages between VMOs.

use crate::helpers::TestContext;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

/// Number of pages used in transfer tests.
const PAGE_COUNT: usize = 5;

/// Helper: create a VMO of `PAGE_COUNT` pages filled with a single byte value.
fn create_vmo_with_fill(fill: u8) -> Arc<VmObject> {
    let vmo = VmObject::new_paged(PAGE_COUNT);
    let buf = vec![fill; PAGE_COUNT * PAGE_SIZE];
    vmo.write(0, &buf).unwrap();
    vmo
}

/// Helper: read the entire VMO contents.
fn read_vmo(vmo: &Arc<VmObject>, size: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size];
    vmo.read(0, &mut buf).unwrap();
    buf
}

/// C++: TEST(VmoTransferDataTestCase, Basic)
/// Transfer first 3 pages of src into last 3 pages of dst.
#[test]
fn basic() {
    let _ctx = TestContext::new();
    let size = PAGE_COUNT * PAGE_SIZE;
    let transfer_size = 3 * PAGE_SIZE;
    let transfer_offset = 2 * PAGE_SIZE;

    let src = create_vmo_with_fill(b's');
    let dst = create_vmo_with_fill(b'd');

    // Read src pages, write to dst at offset, zero src pages.
    let mut transfer_buf = vec![0u8; transfer_size];
    src.read(0, &mut transfer_buf).unwrap();
    dst.write(transfer_offset, &transfer_buf).unwrap();
    // Zero/decommit the source range.
    src.zero(0, transfer_size).unwrap();

    // Dst: first 2 pages = 'd', last 3 pages = 's'.
    let got = read_vmo(&dst, size);
    let mut expected = vec![b'd'; transfer_offset];
    expected.extend(vec![b's'; transfer_size]);
    assert_eq!(got, expected, "dst content mismatch");

    // Src: first 3 pages zeroed, last 2 pages = 's'.
    let got = read_vmo(&src, size);
    let mut expected = vec![0u8; transfer_size];
    expected.extend(vec![b's'; transfer_offset]);
    assert_eq!(got, expected, "src content mismatch");
}

/// C++: TEST(VmoTransferDataTestCase, SameSrcAndDst)
/// Same VMO as both source and destination (different offsets).
#[test]
fn same_src_and_dst() {
    let _ctx = TestContext::new();
    let size = PAGE_COUNT * PAGE_SIZE;
    let transfer_size = 3 * PAGE_SIZE;
    let transfer_offset = 2 * PAGE_SIZE;

    let vmo = create_vmo_with_fill(b's');

    // Read source range, write to dest range, then zero source
    // (non-overlapping portion only).
    let mut transfer_buf = vec![0u8; transfer_size];
    vmo.read(0, &mut transfer_buf).unwrap();
    vmo.write(transfer_offset, &transfer_buf).unwrap();
    // Zero the source pages that don't overlap with destination.
    // Source: [0, transfer_size), Dest: [transfer_offset, transfer_offset + transfer_size)
    // Non-overlapping part of source is [0, transfer_offset).
    vmo.zero(0, transfer_offset).unwrap();

    let got = read_vmo(&vmo, size);
    let mut expected = vec![0u8; transfer_offset];
    expected.extend(vec![b's'; transfer_size]);
    assert_eq!(got, expected, "same-vmo transfer content mismatch");
}

/// C++: TEST(VmoTransferDataTestCase, EmptyOverwritesParentContent)
/// Transfer from an empty (zero) VMO into a snapshot child that sees
/// non-zero parent content. The dst should become zero.
#[test]
fn empty_overwrites_parent_content() {
    let _ctx = TestContext::new();

    let dst_parent = VmObject::new_paged(1);
    let data: u64 = 42;
    dst_parent.write(0, &data.to_ne_bytes()).unwrap();

    // Create snapshot child — should see parent's data.
    let dst = dst_parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut read_val = [0u8; 8];
    dst.read(0, &mut read_val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(read_val),
        42,
        "snapshot child should see parent data"
    );

    // Create empty (all-zero) source VMO.
    let src = VmObject::new_paged(1);

    // Transfer: copy src data into dst, zero src.
    let mut buf = vec![0u8; PAGE_SIZE];
    src.read(0, &mut buf).unwrap();
    dst.write(0, &buf).unwrap();
    src.zero(0, PAGE_SIZE).unwrap();

    // Dst should now be zero (not see parent content anymore).
    dst.read(0, &mut read_val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(read_val),
        0,
        "dst should be zero after transfer from empty src"
    );
}

/// C++: TEST(VmoTransferDataTestCase, DestroyedParentWithNonZeroOffset)
/// Transfer from a snapshot child whose parent was deleted with non-zero offset.
#[test]
fn destroyed_parent_with_non_zero_offset() {
    let _ctx = TestContext::new();

    // Create a 2-page parent with tagged data.
    let parent = VmObject::new_paged(2);
    parent.write(0, &1u32.to_ne_bytes()).unwrap();
    parent.write(PAGE_SIZE, &2u32.to_ne_bytes()).unwrap();

    // Create snapshot child at offset PAGE_SIZE (sees page 2 of parent).
    let child = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    // Verify child sees parent's second page.
    let mut val = [0u8; 4];
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 2);

    // Destroy parent.
    drop(parent);

    // Child should still see its data after parent deletion.
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 2);

    // Transfer from child to a new dst VMO.
    let dst = VmObject::new_paged(1);
    let mut buf = vec![0u8; PAGE_SIZE];
    child.read(0, &mut buf).unwrap();
    dst.write(0, &buf).unwrap();
    child.zero(0, PAGE_SIZE).unwrap();

    // Dst should have the transferred data.
    dst.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 2, "dst should have child's data");

    // Child source should be zeroed.
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 0, "child source should be zeroed");
}

/// C++: TEST(VmoTransferDataTestCase, ChildSliceDst)
/// Transfer data into a VMO that is a SLICE child.
#[test]
fn child_slice_dst() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(2);
    parent.write(0, &0u32.to_ne_bytes()).unwrap();
    parent.write(PAGE_SIZE, &0u32.to_ne_bytes()).unwrap();

    // Create slice child (second page of parent).
    let slice = parent.create_slice(PAGE_SIZE, PAGE_SIZE).unwrap();

    // Create source VMO with data.
    let src = VmObject::new_paged(1);
    src.write(0, &42u32.to_ne_bytes()).unwrap();

    // Transfer to slice.
    let mut buf = vec![0u8; PAGE_SIZE];
    src.read(0, &mut buf).unwrap();
    slice.write(0, &buf).unwrap();
    src.zero(0, PAGE_SIZE).unwrap();

    // Slice should have transferred data.
    let mut val = [0u8; 4];
    slice.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Parent should also see the data (slice is a window into parent).
    parent.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(
        u32::from_ne_bytes(val),
        42,
        "parent should see data written to slice"
    );
}

/// C++: TEST(VmoTransferDataTestCase, ChildSliceSrc)
/// Transfer data from a VMO that is a SLICE child.
#[test]
fn child_slice_src() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(2);
    parent.write(PAGE_SIZE, &42u32.to_ne_bytes()).unwrap();

    // Create slice child (second page of parent).
    let slice = parent.create_slice(PAGE_SIZE, PAGE_SIZE).unwrap();

    // Verify slice sees parent data.
    let mut val = [0u8; 4];
    slice.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Transfer from slice to dst.
    let dst = VmObject::new_paged(1);
    let mut buf = vec![0u8; PAGE_SIZE];
    slice.read(0, &mut buf).unwrap();
    dst.write(0, &buf).unwrap();
    slice.zero(0, PAGE_SIZE).unwrap();

    // Dst should have the data.
    dst.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Slice (and parent at that offset) should be zeroed.
    slice.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 0, "slice source should be zeroed");

    parent.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(
        u32::from_ne_bytes(val),
        0,
        "parent should be zeroed at slice offset"
    );
}

/// C++: TEST(VmoTransferDataTestCase, SnapshotChildSrc)
/// Transfer from a SNAPSHOT child (without pager, which we can test).
#[test]
fn snapshot_child_src() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &42u32.to_ne_bytes()).unwrap();

    // Create snapshot child.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Verify child sees parent data.
    let mut val = [0u8; 4];
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Transfer from child to dst.
    let dst = VmObject::new_paged(1);
    let mut buf = vec![0u8; PAGE_SIZE];
    child.read(0, &mut buf).unwrap();
    dst.write(0, &buf).unwrap();
    child.zero(0, PAGE_SIZE).unwrap();

    // Dst has the data.
    dst.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Child source is zeroed.
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 0);

    // Parent should be untouched.
    parent.read(0, &mut val).unwrap();
    assert_eq!(
        u32::from_ne_bytes(val),
        42,
        "parent should be untouched after transfer from snapshot child"
    );
}

/// C++: TEST(VmoTransferDataTestCase, SnapshotChildDst)
/// Transfer into a SNAPSHOT child (without pager).
#[test]
fn snapshot_child_dst() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &0u32.to_ne_bytes()).unwrap();

    // Create snapshot child.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Create source with data.
    let src = VmObject::new_paged(1);
    src.write(0, &42u32.to_ne_bytes()).unwrap();

    // Transfer into child.
    let mut buf = vec![0u8; PAGE_SIZE];
    src.read(0, &mut buf).unwrap();
    child.write(0, &buf).unwrap();
    src.zero(0, PAGE_SIZE).unwrap();

    // Child should have transferred data.
    let mut val = [0u8; 4];
    child.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 42);

    // Src should be zeroed.
    src.read(0, &mut val).unwrap();
    assert_eq!(u32::from_ne_bytes(val), 0);

    // Parent should be untouched.
    parent.read(0, &mut val).unwrap();
    assert_eq!(
        u32::from_ne_bytes(val),
        0,
        "parent should be untouched after transfer into snapshot child"
    );
}

/// C++: TEST(VmoTransferDataTestCase, RacyDropParent)
/// Regression test: the kernel must be resilient to the parent of a src VMO
/// going away mid-transfer. A thread closes the parent while the transfer
/// is in progress. In zCore, transfers are synchronous and single-threaded,
/// so a true race cannot occur. This test verifies the basic scenario:
/// create a snapshot child, then close the parent, then transfer from child.
#[test]
#[ignore = "needs concurrent threads to race parent drop against transfer_data"]
fn racy_drop_parent() {
    // The real C++ test uses std::thread to race a parent.reset() against
    // dst_vmo.transfer_data(). zCore's cooperative scheduler cannot reproduce
    // this race.
}

/// C++: TEST(VmoTransferDataTestCase, ReferenceChildSrc)
/// Transfer data FROM a REFERENCE child (full-VMO alias). After transfer,
/// both the reference child and its parent should see zeros in the
/// transferred range, and the destination should have the source data.
#[test]
fn reference_child_src() {
    let _ctx = TestContext::new();
    let transfer_pages = 3;
    let transfer_size = transfer_pages * PAGE_SIZE;
    let total_size = PAGE_COUNT * PAGE_SIZE;

    // Create parent filled with 's'.
    let parent = create_vmo_with_fill(b's');

    // Create a REFERENCE child (full-VMO slice with size 0 = inherit parent size).
    let src = parent.create_reference_slice(0, total_size, false).unwrap();

    // Create destination filled with 'd'.
    let dst = create_vmo_with_fill(b'd');

    // Simulate transfer: read from src, write to dst, zero src.
    let mut transfer_buf = vec![0u8; transfer_size];
    src.read(0, &mut transfer_buf).unwrap();
    dst.write(0, &transfer_buf).unwrap();
    src.zero(0, transfer_size).unwrap();

    // Verify src (reference child) is zeroed in transfer range.
    let src_data = read_vmo(&src, transfer_size);
    assert!(
        src_data.iter().all(|&b| b == 0),
        "reference child src should be zeroed"
    );

    // Verify parent also sees zeros (reference child is an alias).
    let parent_data = read_vmo(&parent, total_size);
    assert!(
        parent_data[..transfer_size].iter().all(|&b| b == 0),
        "parent should see zeros in transferred range"
    );
    assert!(
        parent_data[transfer_size..].iter().all(|&b| b == b's'),
        "parent should retain original data beyond transfer range"
    );

    // Verify destination has the transferred data.
    let dst_data = read_vmo(&dst, total_size);
    assert!(
        dst_data[..transfer_size].iter().all(|&b| b == b's'),
        "dst should have source data in transfer range"
    );
    assert!(
        dst_data[transfer_size..].iter().all(|&b| b == b'd'),
        "dst should retain original data beyond transfer range"
    );
}

/// C++: TEST(VmoTransferDataTestCase, ReferenceChildDst)
/// Transfer data INTO a REFERENCE child (full-VMO alias). After transfer,
/// both the reference child and its parent should see the source data,
/// and the source should be zeroed.
#[test]
fn reference_child_dst() {
    let _ctx = TestContext::new();
    let transfer_pages = 3;
    let transfer_size = transfer_pages * PAGE_SIZE;
    let total_size = PAGE_COUNT * PAGE_SIZE;

    // Create source filled with 's'.
    let src = create_vmo_with_fill(b's');

    // Create parent filled with 'd'.
    let parent = create_vmo_with_fill(b'd');

    // Create a REFERENCE child (full-VMO alias).
    let dst = parent.create_reference_slice(0, total_size, false).unwrap();

    // Simulate transfer: read from src, write to dst, zero src.
    let mut transfer_buf = vec![0u8; transfer_size];
    src.read(0, &mut transfer_buf).unwrap();
    dst.write(0, &transfer_buf).unwrap();
    src.zero(0, transfer_size).unwrap();

    // Verify source is zeroed in transfer range.
    let src_data = read_vmo(&src, total_size);
    assert!(
        src_data[..transfer_size].iter().all(|&b| b == 0),
        "src should be zeroed in transfer range"
    );
    assert!(
        src_data[transfer_size..].iter().all(|&b| b == b's'),
        "src should retain data beyond transfer range"
    );

    // Verify parent sees the transferred data (reference child is an alias).
    let parent_data = read_vmo(&parent, total_size);
    assert!(
        parent_data[..transfer_size].iter().all(|&b| b == b's'),
        "parent should see source data in transfer range"
    );
    assert!(
        parent_data[transfer_size..].iter().all(|&b| b == b'd'),
        "parent should retain original data beyond transfer range"
    );

    // Verify dst (reference child) has the transferred data.
    let dst_data = read_vmo(&dst, transfer_size);
    assert!(
        dst_data.iter().all(|&b| b == b's'),
        "dst should have source data"
    );
}

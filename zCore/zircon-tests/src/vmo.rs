//! VMO syscall tests.
//!
//! Mirrors tests from zircon/system/utest/core/vmo/.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// vmo_create: basic creation with correct size.
#[test]
fn vmo_create() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(4);
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);
}

/// vmo_read/write: basic data round-trip.
#[test]
fn vmo_read_write() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    let data = b"hello vmo!";
    vmo.write(0, data).unwrap();

    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, data);
}

/// vmo_read: reading unwritten pages returns zeros.
#[test]
fn vmo_read_zeros() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    let mut buf = [0xFFu8; 64];
    vmo.read(0, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

/// vmo_get_size: reports correct size.
#[test]
fn vmo_get_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(3);
    assert_eq!(vmo.len(), 3 * PAGE_SIZE);
}

/// vmo_set_size: resize a resizable VMO.
#[test]
fn vmo_set_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 2);
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);

    vmo.set_len(4 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);

    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);
}

/// vmo_set_size: non-resizable VMO returns error.
#[test]
fn vmo_set_size_not_resizable() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    assert!(vmo.set_len(4 * PAGE_SIZE).is_err());
}

/// vmo_create_child (SNAPSHOT): COW clone isolation.
#[test]
fn vmo_snapshot_isolation() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Child sees parent data
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to parent doesn't affect child (COW)
    parent.write(0, &[0xBB]).unwrap();

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA); // still old value

    // Write to child doesn't affect parent
    child.write(0, &[0xCC]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB); // unchanged
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xCC);
}

/// vmo_create_child (SLICE): transparent write-through.
#[test]
fn vmo_slice_write_through() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // Slice sees parent data
    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to slice is visible in parent
    slice.write(0, &[0xBB]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    // Write to parent is visible in slice
    parent.write(0, &[0xCC]).unwrap();
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xCC);
}

/// vmo_op_range (COMMIT): committed bytes increase.
#[test]
fn vmo_commit() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    let info = vmo.get_info();
    assert_eq!(info.committed_bytes, 0);

    // Write commits a page
    vmo.write(0, &[42]).unwrap();
    let info = vmo.get_info();
    assert_eq!(info.committed_bytes as usize, PAGE_SIZE);
}

/// vmo_op_range (DECOMMIT): releases pages.
#[test]
fn vmo_decommit() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    vmo.decommit(0, PAGE_SIZE).unwrap();
    assert_eq!(vmo.get_info().committed_bytes, 0);

    // Reading after decommit returns zeros
    let mut buf = [0xFFu8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// VMO zero operation.
#[test]
fn vmo_zero() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"Hello World!").unwrap();
    vmo.zero(0, 5).unwrap();

    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf[0..5], &[0, 0, 0, 0, 0]);
    assert_eq!(&buf[5..12], b" World!");
}

/// Slice populated_bytes should be zero (pages attributed to parent).
#[test]
fn vmo_slice_attributed_counts() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[42]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // Parent owns the page
    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);

    // Slice does NOT own the page
    let slice_info = slice.get_info();
    assert_eq!(slice_info.populated_bytes, 0);
}

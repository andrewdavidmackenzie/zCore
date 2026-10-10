//! VMO slice tests.
//!
//! Mirrors VmoSliceTestCase from core-tests.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

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

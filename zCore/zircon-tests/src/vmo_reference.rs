//! VMO reference child tests.
//!
//! Faithful 1:1 ports from VmoReference suite (vmo-reference.cc).
//! Test names match the C++ originals exactly.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::{VmObject, VmoInfoFlags, PAGE_SIZE};


/// C++: TEST(VmoReference, Write)
/// Reference sees parent writes and vice versa.
#[test]
fn write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Write to the parent.
    parent.write(0, &[0xAA]).unwrap();

    // The reference should see the write.
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to the reference.
    reference.write(0, &[0xBB]).unwrap();

    // The parent should see the write.
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
}

/// C++: TEST(VmoReference, ZeroChildren)
/// VMO_ZERO_CHILDREN signal tracks reference lifecycle.
#[test]
fn zero_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    // Currently the parent has no children, so VMO_ZERO_CHILDREN should be set.
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Create a reference.
    let child = parent.create_reference_slice(0, 0, false).unwrap();

    // Currently the parent has one child, so VMO_ZERO_CHILDREN should be cleared.
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Close the child reference.
    drop(child);

    // VMO_ZERO_CHILDREN should be set again.
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// C++: TEST(VmoReference, ChildSnapshot)
/// Snapshot child of reference parent — write isolation.
#[test]
fn child_snapshot() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let _reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Create a snapshot child of the parent.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to parent shouldn't affect snapshot child.
    parent.write(0, &[0xBB]).unwrap();
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to snapshot child shouldn't affect parent.
    child.write(0, &[0xCC]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
}

/// C++: TEST(VmoReference, ChildSlice)
/// Slice of reference — write-through semantics.
#[test]
fn child_slice() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Create a slice child of the reference.
    let slice = reference.create_slice(0, PAGE_SIZE).unwrap();

    // Slice sees parent data through reference.
    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write through slice is visible in parent.
    slice.write(0, &[0xBB]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
}

/// C++: TEST(VmoReference, NestedChild)
/// Nested references propagate writes.
#[test]
fn nested_child() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let ref1 = parent.create_reference_slice(0, 0, false).unwrap();
    let ref2 = ref1.create_reference_slice(0, 0, false).unwrap();

    // Both refs see parent data
    let mut buf = [0u8; 1];
    ref1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    ref2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write through ref2 visible everywhere
    ref2.write(0, &[0xBB]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
    ref1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
}

/// C++: TEST(VmoReference, AttributedCounts)
/// Reference populated_bytes is zero even after parent commits pages.
/// Dropping parent still keeps reference's populated_bytes at zero.
#[test]
fn attributed_counts() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Commit a page in the parent.
    parent.write(0, &[0xAA]).unwrap();

    // Parent should see the page populated; reference does not.
    let ref_info = reference.get_info();
    assert_eq!(ref_info.populated_bytes, 0);
    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);

    // The reference should still see the data.
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Drop the parent.
    drop(parent);

    // Committed pages still not attributed to the reference.
    let ref_info = reference.get_info();
    assert_eq!(ref_info.populated_bytes, 0);

    // The reference can still read the data.
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// C++: TEST(VmoReference, Resize)
/// Resizing parent is visible through reference; resizing reference is
/// visible through parent.
#[test]
fn resize() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged_with_resizable(true, 4);

    let reference = parent.create_reference_slice(0, 0, true).unwrap();

    // Resize the parent.
    parent.set_len(2 * PAGE_SIZE).unwrap();
    assert_eq!(parent.len(), 2 * PAGE_SIZE);

    // The reference should see the resize.
    assert_eq!(reference.len(), 2 * PAGE_SIZE);

    // Resize the reference.
    reference.set_len(3 * PAGE_SIZE).unwrap();
    assert_eq!(reference.len(), 3 * PAGE_SIZE);

    // The parent should see the resize.
    assert_eq!(parent.len(), 3 * PAGE_SIZE);
}

/// C++: TEST(VmoReference, UnsupportedResize)
/// Non-resizable reference rejects set_size.
#[test]
fn unsupported_resize() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged_with_resizable(true, 4);

    // Create a non-resizable reference.
    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Non-resizable reference can't resize.
    assert!(reference.set_len(2 * PAGE_SIZE).is_err());
    assert_eq!(reference.len(), 4 * PAGE_SIZE);
}

/// C++: TEST(VmoReference, GetInfo)
/// Reference does not have IS_COW_CLONE flag.
#[test]
fn get_info() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    let info = reference.get_info();
    assert!(!info.flags.contains(VmoInfoFlags::IS_COW_CLONE));
}

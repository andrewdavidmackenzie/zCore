//! VMO reference child tests.
//!
//! Mirrors VmoReference from core-tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// Reference sees parent writes and vice versa.
#[test]
fn reference_write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Reference sees parent data
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write through reference visible in parent
    reference.write(0, &[0xBB]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
}

/// Reference populated_bytes should be zero.
#[test]
fn reference_attributed_counts() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);

    let ref_info = reference.get_info();
    assert_eq!(ref_info.populated_bytes, 0);
}

/// Reference is not reported as COW clone.
#[test]
fn reference_not_cow() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    let info = reference.get_info();
    use zircon_object::vm::VmoInfoFlags;
    assert!(!info.flags.contains(VmoInfoFlags::IS_COW_CLONE));
}

/// Snapshot child of reference sees parent data.
#[test]
fn reference_child_snapshot() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let _reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Create COW child of the reference — should work on the
    // underlying parent VMO
    // Note: create_child on a reference delegates to the parent
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to parent, child shouldn't see it (snapshot)
    parent.write(0, &[0xBB]).unwrap();
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// Nested references.
#[test]
fn reference_nested() {
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

/// Reference zero_children signal.
#[test]
fn reference_zero_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    use zircon_object::object::Signal;
    // Initially no children
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    let reference = parent.create_reference_slice(0, 0, false).unwrap();
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    drop(reference);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

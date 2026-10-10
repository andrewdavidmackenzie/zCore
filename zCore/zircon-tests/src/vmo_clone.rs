//! VMO clone (SNAPSHOT) tests.
//!
//! Mirrors VmoCloneTestCase and VmoClone2TestCase from core-tests.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// Snapshot clone sees parent data at creation time.
#[test]
fn clone_sees_parent_data() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// Write to parent after clone doesn't affect child.
#[test]
fn clone_parent_write_isolation() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    parent.write(0, &[0xBB]).unwrap();

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA); // child still sees old value
}

/// Write to child doesn't affect parent.
#[test]
fn clone_child_write_isolation() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    child.write(0, &[0xCC]).unwrap();

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA); // parent unchanged

    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xCC);
}

/// Multiple clones are independent.
#[test]
fn clone_multiple_independent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child1 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    child1.write(0, &[0x11]).unwrap();
    child2.write(0, &[0x22]).unwrap();

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    child1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0x11);
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0x22);
}

/// Clone of a clone (grandchild).
#[test]
fn clone_grandchild() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let grandchild = child.create_child(false, 0, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 1];
    grandchild.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to grandchild doesn't affect parent or child
    grandchild.write(0, &[0xCC]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// Clone with offset.
#[test]
fn clone_with_offset() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);
    for i in 0..4u8 {
        parent.write(i as usize * PAGE_SIZE, &[i + 1]).unwrap();
    }

    // Clone pages 1-3 (offset = PAGE_SIZE, len = 3*PAGE_SIZE)
    let child = parent
        .create_child(false, PAGE_SIZE, 3 * PAGE_SIZE)
        .unwrap();

    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 2); // page 1 of parent

    child.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf[0], 3); // page 2 of parent

    child.read(2 * PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf[0], 4); // page 3 of parent
}

/// committed_bytes reflects COW state.
#[test]
fn clone_committed_bytes() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[1]).unwrap();
    parent.write(PAGE_SIZE, &[2]).unwrap();

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Child should see 2 committed pages (from parent)
    let info = child.get_info();
    assert_eq!(info.committed_bytes as usize, 2 * PAGE_SIZE);

    // Write to child creates a local COW copy
    child.write(0, &[0xFF]).unwrap();
    let info = child.get_info();
    assert_eq!(info.committed_bytes as usize, 2 * PAGE_SIZE);
}

//! VMO clone (SNAPSHOT) tests.
//!
//! Mirrors VmoCloneTestCase and VmoClone2TestCase from core-tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::{VmObject, VmoInfoFlags, PAGE_SIZE};
use zircon_object::ZxError;

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

/// Clone is not resizable by default.
#[test]
fn clone_not_resizable_by_default() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    assert!(!child.is_resizable());
    assert!(child.set_len(PAGE_SIZE).is_err());
}

/// Clone can be resizable if requested.
#[test]
fn clone_resizable() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    let child = parent.create_child(true, 0, 2 * PAGE_SIZE).unwrap();
    assert!(child.is_resizable());

    child.set_len(PAGE_SIZE).unwrap();
    assert_eq!(child.len(), PAGE_SIZE);
}

/// Clone has IS_COW_CLONE flag set.
#[test]
fn clone_is_cow_flag() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let info = child.get_info();
    assert!(info.flags.contains(zircon_object::vm::VmoInfoFlags::IS_COW_CLONE));

    // Parent does NOT have IS_COW_CLONE
    let parent_info = parent.get_info();
    assert!(!parent_info.flags.contains(zircon_object::vm::VmoInfoFlags::IS_COW_CLONE));
}

/// Dropping child restores VMO_ZERO_CHILDREN on parent.
#[test]
fn clone_zero_children_signal() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    drop(child);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// Multiple clones: signal only restored when ALL children dropped.
#[test]
fn clone_zero_children_all_dropped() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    let child1 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    drop(child1);
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN)); // still has child2

    drop(child2);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// Clone reads zeros beyond parent's written region.
#[test]
fn clone_reads_zeros_beyond_parent_data() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA]).unwrap();
    // Page 1 is written, page 2 is not

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    let mut buf = [0xFFu8; 1];
    child.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf[0], 0); // unwritten page reads as zero
}

/// Clone name and type_name.
#[test]
fn clone_name() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    child.set_name("my-clone");
    assert_eq!(child.name(), "my-clone");
    assert_eq!(child.type_name(), "VmObject");
}

// -- Faithful 1:1 ports from VmoCloneTestCase (vmo-clone.cc) --
// Test names match the C++ originals exactly.

/// C++: TEST(VmoCloneTestCase, Decommit)
/// Decommit is not supported on clones or parent VMOs with children.
/// Once the clone is closed, decommit should work again.
#[test]
fn decommit() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    let child = parent.create_child(false, 0, 4 * PAGE_SIZE).unwrap();

    // Decommit is not supported on clones (has parent)
    assert_eq!(
        child.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED
    );

    // Decommit is not supported on parent VMOs which have children
    assert_eq!(
        parent.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED
    );

    // Close the clone — decommit should now work on parent
    drop(child);
    parent.decommit(0, PAGE_SIZE).unwrap();
}

/// C++: TEST(VmoCloneTestCase, NoResize)
/// Non-resizable snapshot clone rejects set_size.
#[test]
fn no_resize() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    let child = parent.create_child(false, 0, 4 * PAGE_SIZE).unwrap();

    // Can't grow
    assert_eq!(
        child.set_len(5 * PAGE_SIZE).unwrap_err(),
        ZxError::UNAVAILABLE
    );

    // Can't shrink
    assert_eq!(
        child.set_len(3 * PAGE_SIZE).unwrap_err(),
        ZxError::UNAVAILABLE
    );

    // Size unchanged
    assert_eq!(child.len(), 4 * PAGE_SIZE);
}

/// C++: TEST(VmoCloneTestCase, NameProperty)
/// Parent name propagates to child clone.
#[test]
fn name_property() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);
    parent.set_name("test1");

    let child = parent.create_child(false, 0, 4 * PAGE_SIZE).unwrap();

    // Child should inherit parent's name
    assert_eq!(child.name(), "test1");

    // Setting child name doesn't affect parent
    child.set_name("clone-name");
    assert_eq!(child.name(), "clone-name");
    assert_eq!(parent.name(), "test1");
}

/// C++: TEST(VmoCloneTestCase, ImmutableClone)
/// SNAPSHOT + set_immutable creates an immutable child.
#[test]
fn immutable_clone() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    let child = parent.create_child(false, 0, 4 * PAGE_SIZE).unwrap();
    child.set_immutable();

    let info = child.get_info();
    assert!(info.flags.contains(VmoInfoFlags::IMMUTABLE));
}

/// C++: TEST(VmoCloneTestCase, NotImmutableMissingNoWrite)
/// Plain SNAPSHOT (without set_immutable) is not immutable.
#[test]
fn not_immutable_missing_no_write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    let child = parent.create_child(false, 0, 4 * PAGE_SIZE).unwrap();

    let info = child.get_info();
    assert!(!info.flags.contains(VmoInfoFlags::IMMUTABLE));
}

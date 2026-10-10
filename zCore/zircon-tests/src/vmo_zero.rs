//! VMO zero operation tests.
//!
//! Faithful 1:1 ports from VmoZeroTestCase (vmo-zero.cc).
//! Test names match the C++ originals exactly.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// C++: TEST(VmoZeroTestCase, UnalignedUnCommitted)
/// Zero across page boundaries on uncommitted pages should not commit pages.
#[test]
fn unaligned_uncommitted() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);

    assert_eq!(vmo.get_info().populated_bytes, 0);

    // Zero across both page boundaries. Already zero pages — should not commit.
    vmo.zero(PAGE_SIZE / 2, PAGE_SIZE).unwrap();

    assert_eq!(vmo.get_info().populated_bytes, 0);
}

/// C++: TEST(VmoZeroTestCase, ContentInParentAndChild)
/// Zero a COW child page that has forked content; child should read zeros.
#[test]
fn content_in_parent_and_child() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    // Write to first page of parent
    parent.write(0, &[1]).unwrap();

    // Create a SNAPSHOT child of both pages, then fork page 0
    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    child.write(0, &[2]).unwrap();

    // Zero the first page of the child
    child.zero(0, PAGE_SIZE).unwrap();

    // Child should now read zeros on page 0
    let mut buf = [0xFFu8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// C++: TEST(VmoZeroTestCase, ZeroLengths)
/// Zero-length zero operations succeed.
#[test]
fn zero_lengths() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    vmo.zero(0, 0).unwrap();
    vmo.zero(10, 0).unwrap();
    vmo.zero(PAGE_SIZE, 0).unwrap();
}

/// C++: TEST(VmoZeroTestCase, EmptyCowChildren)
/// Zero a COW child, then zero parent — both see zero pages.
#[test]
fn empty_cow_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);

    // Write to page 0 of parent
    parent.write(0, &[1]).unwrap();

    // Create SNAPSHOT child
    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Child should see parent's data
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 1);

    // Zero the child's page 0
    child.zero(0, PAGE_SIZE).unwrap();

    // Child should now read zeros
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);

    // Parent should still see 1
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 1);

    // Zero parent's page 0
    parent.zero(0, PAGE_SIZE).unwrap();

    // Both should now be zero
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// C++: TEST(VmoZeroTestCase, ContentInParentAndChild) — verify parent unaffected
/// Zero child page 0, parent should still have its data.
#[test]
fn content_in_parent_preserved() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[42]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Child sees parent data
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);

    // Zero child
    child.zero(0, PAGE_SIZE).unwrap();
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);

    // Parent unaffected
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 42);
}

/// C++: TEST(VmoZeroTestCase, ChildZeroThenWrite)
/// Zero child page, then write to it — write should succeed.
#[test]
fn child_zero_then_write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Zero the child
    child.zero(0, PAGE_SIZE).unwrap();

    // Write new data to the zeroed page
    child.write(0, &[0xBB]).unwrap();

    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    // Parent unchanged
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// C++: TEST(VmoZeroTestCase, Nested)
/// Zero parent with two children — both children preserve their data.
#[test]
fn nested() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child1 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Both children see parent data
    let mut buf = [0u8; 1];
    child1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Zero parent
    parent.zero(0, PAGE_SIZE).unwrap();

    // Parent reads zeros
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);

    // Children still see their snapshot data
    child1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

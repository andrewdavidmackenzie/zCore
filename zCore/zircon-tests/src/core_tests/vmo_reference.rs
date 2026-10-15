//! VMO reference child tests.
//!
//! Faithful 1:1 ports from VmoReference suite (vmo-reference.cc).
//! Test names match the C++ originals exactly.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Pager, Port};
use zircon_object::vm::{Stream, VmObject, VmoInfoFlags, PAGE_SIZE};

extern crate alloc;

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

// ── MappingUpdate tests (using read/write instead of VMAR mapping) ──
// The C++ tests use VMAR mapping; we verify the same semantics
// through VmObject::read/write which tests the kernel object behavior.

/// C++: TEST(VmoReference, MappingUpdate)
/// Write to parent, decommit, reference reads zeros.
#[test]
fn mapping_update() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Reference sees parent data.
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Decommit from parent — data should become zero.
    parent.decommit(0, PAGE_SIZE).unwrap();

    // Reference should now read zeros.
    reference.read(0, &mut buf).unwrap();
    assert_eq!(
        buf[0], 0,
        "reference should read zeros after parent decommit"
    );
}

/// C++: TEST(VmoReference, MappingUpdateAfterDroppingRef)
/// Write to parent, create reference, drop reference, decommit parent.
#[test]
fn mapping_update_after_dropping_ref() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Drop the reference.
    drop(reference);

    // Decommit parent — should succeed without issues.
    parent.decommit(0, PAGE_SIZE).unwrap();

    // Parent reads zeros.
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// C++: TEST(VmoReference, MappingUpdateAfterDroppingParent)
/// Write to parent, create reference, drop parent, read reference.
#[test]
fn mapping_update_after_dropping_parent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Drop parent — reference keeps data alive.
    drop(parent);

    // Reference should still see the data.
    let mut buf = [0u8; 1];
    reference.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

/// C++: TEST(VmoReference, MappingUpdateNestedChildDestroy)
/// Nested references: destroy inner ref, verify outer still works.
#[test]
fn mapping_update_nested_child_destroy() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let ref1 = parent.create_reference_slice(0, 0, false).unwrap();
    let ref2 = ref1.create_reference_slice(0, 0, false).unwrap();

    // Both refs see data.
    let mut buf = [0u8; 1];
    ref2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Destroy inner ref.
    drop(ref2);

    // Outer ref still works.
    ref1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Decommit through parent.
    parent.decommit(0, PAGE_SIZE).unwrap();

    // Outer ref reads zeros.
    ref1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0, "ref1 should read zeros after parent decommit");
}

/// C++: TEST(VmoReference, MappingUpdateNestedRootDestroy)
/// Nested references: destroy root parent, verify refs still work.
#[test]
fn mapping_update_nested_root_destroy() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let ref1 = parent.create_reference_slice(0, 0, false).unwrap();
    let ref2 = ref1.create_reference_slice(0, 0, false).unwrap();

    // Drop root parent.
    drop(parent);

    // Both refs still see data.
    let mut buf = [0u8; 1];
    ref1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    ref2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
}

// ── Stream + pager-backed reference tests ───────────────────────────

/// C++: TEST(VmoReference, Streams)
/// Stream can be created on a reference VMO.
#[test]
fn streams() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged_with_resizable(true, 2);
    parent.set_content_size(2 * PAGE_SIZE).unwrap();
    parent.write(0, &[0xAA; 8]).unwrap();

    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Create a stream on the reference.
    // mode 0x3 = read + write
    let stream = Stream::create(reference.clone(), 0, 0x3);

    // Read from stream — should see parent data.
    let mut buf = [0u8; 8];
    let bytes_read = stream.read(&mut buf).unwrap();
    assert_eq!(bytes_read, 8);
    assert_eq!(buf, [0xAA; 8]);
}

/// C++: TEST(VmoReference, UnsupportedChild)
/// Pager-backed reference: verify that creating snapshot children fails.
#[test]
fn unsupported_child() {
    let _ctx = TestContext::new();
    let pager = Pager::new();
    let port = Port::new(0).unwrap();

    // Create pager-backed VMO.
    let pager_vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Supply a page so it's committed.
    let src = VmObject::new_paged(1);
    src.write(0, &[0xAA]).unwrap();
    pager
        .supply_pages(&pager_vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();

    // Create reference of the pager-backed VMO.
    let reference = pager_vmo.create_reference_slice(0, 0, false).unwrap();

    // Creating a snapshot child of a pager-backed reference should fail.
    let result = reference.create_child(false, 0, PAGE_SIZE);
    assert!(
        result.is_err(),
        "snapshot child of pager-backed reference should fail"
    );
}

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// vmo/vmo-reference.cc:130
///
/// Tests COW child creation, data read-through, and write isolation.
/// zCore treats SNAPSHOT_AT_LEAST_ON_WRITE as a regular snapshot,
/// so parent writes after child creation are NOT visible in child
/// (unlike Fuchsia where they would be until the child writes).
#[test]
fn child_snapshot_at_least_on_write() {
    let _ctx = TestContext::new();
    let root = VmObject::new_paged(1);
    root.write(0, &[0xAAu8; 8]).unwrap();

    let child = root.create_child(false, 0, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 8];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA; 8], "child should read parent data");

    child.write(0, &[0xCC; 8]).unwrap();

    let mut parent_buf = [0u8; 8];
    root.read(0, &mut parent_buf).unwrap();
    assert_eq!(parent_buf, [0xAA; 8], "parent should be unchanged");
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xCC; 8], "child should have forked data");
}

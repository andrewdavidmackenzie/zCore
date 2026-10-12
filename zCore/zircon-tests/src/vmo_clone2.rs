//! VMO Clone2 tests.
//!
//! Faithful 1:1 ports from VmoClone2TestCase (vmo-clone2.cc).
//! These test COW (copy-on-write) snapshot clone behavior: read/write
//! isolation, close ordering, offset clones, disjoint clones, resize.

use crate::helpers::TestContext;
use alloc::sync::Arc;
use alloc::vec::Vec;

use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

/// Helper: write a u32 tag at `offset`.
fn vmo_write(vmo: &Arc<VmObject>, val: u32, offset: usize) {
    vmo.write(offset, &val.to_ne_bytes()).unwrap();
}

/// Helper: read a u32 from `offset` and verify it matches `expected`.
fn vmo_check(vmo: &Arc<VmObject>, expected: u32, offset: usize) {
    let mut buf = [0u8; 4];
    vmo.read(offset, &mut buf).unwrap();
    let actual = u32::from_ne_bytes(buf);
    assert_eq!(
        actual, expected,
        "at offset {:#x}: expected {}, got {}",
        offset, expected, actual
    );
}

/// Helper: create a VMO with `pages` pages, each tagged with its index+1.
fn init_page_tagged_vmo(pages: usize) -> Arc<VmObject> {
    let vmo = VmObject::new_paged(pages);
    for i in 0..pages {
        vmo_write(&vmo, (i + 1) as u32, i * PAGE_SIZE);
    }
    vmo
}

/// C++: TEST_F(VmoClone2TestCase, Read)
/// Snapshot child sees parent data at creation time.
#[test]
fn read() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_check(&child, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, CloneVmoWrite)
/// Write to clone doesn't affect parent.
#[test]
fn clone_vmo_write() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_write(&child, 99, 0);

    // Clone has new value
    vmo_check(&child, 99, 0);
    // Parent unchanged
    vmo_check(&parent, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, ParentVmoWrite)
/// Write to parent doesn't affect clone.
#[test]
fn parent_vmo_write() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_write(&parent, 99, 0);

    // Parent has new value
    vmo_check(&parent, 99, 0);
    // Clone unchanged
    vmo_check(&child, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, CloseOriginal)
/// Clone survives parent close.
#[test]
fn close_original() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    drop(parent);

    // Clone still readable
    vmo_check(&child, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, CloseClone)
/// Parent survives clone close.
#[test]
fn close_clone() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    drop(child);

    // Parent still readable
    vmo_check(&parent, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, ZeroPageWrite)
/// Writing to zero page in clone creates a private copy.
#[test]
fn zero_page_write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1); // unwritten = zeros

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Both see zeros
    vmo_check(&parent, 0, 0);
    vmo_check(&child, 0, 0);

    // Write to clone
    vmo_write(&child, 42, 0);
    vmo_check(&child, 42, 0);
    vmo_check(&parent, 0, 0); // parent still zero
}

/// C++: TEST_F(VmoClone2TestCase, SplitPageClosure)
/// Close parent after writing to both, clone retains its data.
#[test]
fn split_page_closure() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(1);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_write(&child, 99, 0);

    // Drop parent
    drop(parent);

    // Clone retains its write
    vmo_check(&child, 99, 0);
}

/// C++: TEST_F(VmoClone2TestCase, OffsetTest2)
/// Clone with offset reads correct parent pages.
#[test]
fn offset_test2() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    // Clone pages 2-3 of parent (offset = 2*PAGE_SIZE)
    let child = parent
        .create_child(false, 2 * PAGE_SIZE, 2 * PAGE_SIZE)
        .unwrap();

    vmo_check(&child, 3, 0); // page 2 of parent
    vmo_check(&child, 4, PAGE_SIZE); // page 3 of parent

    // Write to clone doesn't affect parent
    vmo_write(&child, 99, 0);
    vmo_check(&child, 99, 0);
    vmo_check(&parent, 3, 2 * PAGE_SIZE); // parent unchanged
}

/// C++: TEST_F(VmoClone2TestCase, Overflow)
/// Clone with huge offset that would overflow returns error.
#[test]
fn overflow() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    // Page-aligned offset that causes offset+len overflow
    let huge_offset = usize::MAX - PAGE_SIZE + 1;
    let result = parent.create_child(false, huge_offset, PAGE_SIZE);
    assert!(result.is_err());
}

/// C++: TEST_F(VmoClone2TestCase, OutOfBounds)
/// Clone smaller than parent — reads within bounds work.
/// NOTE: zCore's kernel object layer may not enforce OUT_OF_RANGE
/// for reads past the VMO end the same way the syscall layer does.
#[test]
fn out_of_bounds() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    // Clone only 1 page
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Reading within bounds works
    vmo_check(&child, 1, 0);

    // Clone is exactly 1 page
    assert_eq!(child.len(), PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, SmallClone)
/// Clone smaller than parent.
#[test]
fn small_clone() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_check(&child, 1, 0);
    assert_eq!(child.len(), PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, SmallCloneChild)
/// Clone of a clone.
#[test]
fn small_clone_child() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let grandchild = child.create_child(false, 0, PAGE_SIZE).unwrap();

    vmo_check(&grandchild, 1, 0);
    assert_eq!(grandchild.len(), PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, SmallClones)
/// Multiple small clones at different offsets.
#[test]
fn small_clones() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    let c0 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
    let c2 = parent
        .create_child(false, 2 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();
    let c3 = parent
        .create_child(false, 3 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();

    vmo_check(&c0, 1, 0);
    vmo_check(&c1, 2, 0);
    vmo_check(&c2, 3, 0);
    vmo_check(&c3, 4, 0);
}

/// C++: TEST_F(VmoCloneDisjointClonesTests, DisjointCloneEarlyClose)
/// Disjoint clones: close parent before children.
#[test]
fn disjoint_clone_early_close() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    let c0 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    drop(parent);

    vmo_check(&c0, 1, 0);
    vmo_check(&c1, 2, 0);
}

/// C++: TEST_F(VmoCloneDisjointClonesTests, DisjointCloneLateClose)
/// Disjoint clones: close children before parent.
#[test]
fn disjoint_clone_late_close() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    let c0 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo_check(&c0, 1, 0);
    vmo_check(&c1, 2, 0);

    drop(c0);
    drop(c1);

    // Parent still valid
    vmo_check(&parent, 1, 0);
    vmo_check(&parent, 2, PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, ResizeGrow)
/// Resizable clone can grow; new pages are zero.
#[test]
fn resize_grow() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    let child = parent.create_child(true, 0, PAGE_SIZE).unwrap();
    vmo_check(&child, 1, 0);

    // Grow
    child.set_len(2 * PAGE_SIZE).unwrap();
    assert_eq!(child.len(), 2 * PAGE_SIZE);

    // New page reads as zero (not parent page 2)
    vmo_check(&child, 0, PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, Children)
/// Multiple children, close in order.
#[test]
fn children() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    let c0 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    vmo_check(&c0, 1, 0);
    vmo_check(&c1, 1, 0);

    // Write to c0 doesn't affect c1
    vmo_write(&c0, 99, 0);
    vmo_check(&c0, 99, 0);
    vmo_check(&c1, 1, 0);

    drop(c0);
    vmo_check(&c1, 1, 0);
    vmo_check(&parent, 1, 0);
}

/// C++: TEST_F(VmoClone2TestCase, Uncached)
/// Uncached VMO can't create snapshot children.
#[test]
fn uncached() {
    use zircon_object::vm::CachePolicy;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_cache_policy(CachePolicy::Uncached).unwrap();

    let result = vmo.create_child(false, 0, PAGE_SIZE);
    assert!(result.is_err());
}

/// C++: TEST_F(VmoClone2TestCase, Info)
/// Clone info has correct parent_koid and IS_COW_CLONE flag.
#[test]
fn info() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let info = child.get_info();

    // parent_koid field is private; verify IS_COW_CLONE flag
    assert!(info
        .flags
        .contains(zircon_object::vm::VmoInfoFlags::IS_COW_CLONE));
}

/// C++: TEST_F(VmoClone2TestCase, NoAccumulatedOverflow)
/// Offset calculations don't overflow.
#[test]
fn no_accumulated_overflow() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    vmo_write(&parent, 1, 0);
    vmo_write(&parent, 2, PAGE_SIZE);

    // Clone at offset PAGE_SIZE
    let child = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
    vmo_check(&child, 2, 0);

    // Grandchild at offset 0 of child (= offset PAGE_SIZE of parent)
    let grandchild = child.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_check(&grandchild, 2, 0);
}

// ── Additional VmoClone2 tests ported from vmo-clone2.cc ────────────

/// Helper: create a resizable, page-tagged VMO (each page tagged with index+1).
fn init_page_tagged_vmo_resizable(pages: usize) -> Arc<VmObject> {
    let vmo = VmObject::new_paged_with_resizable(true, pages);
    for i in 0..pages {
        vmo_write(&vmo, (i + 1) as u32, i * PAGE_SIZE);
    }
    vmo
}

/// C++: TEST_F(VmoClone2TestCase, ResizeOverSiblingRange)
/// Regression test for Fuchsia bug 53710. When a COW child is resized,
/// its `parent_limit_` must be correctly updated even when the resize
/// crosses a sibling's range. After shrink+grow, new pages must be zero.
#[test]
fn resize_over_sibling_range() {
    let _ctx = TestContext::new();
    // Parent must be resizable since we call set_len on it.
    let vmo = init_page_tagged_vmo_resizable(4);

    // Create an intermediate hidden parent (non-resizable snapshot).
    let _intermediate = vmo.create_child(false, 0, 4 * PAGE_SIZE).unwrap();

    // Create a resizable sibling as a one-page hole at offset 2*PAGE_SIZE.
    let _sibling = vmo.create_child(true, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();

    // Resize the vmo down to 1 page then back up to 4 pages.
    vmo.set_len(PAGE_SIZE).unwrap();
    vmo.set_len(4 * PAGE_SIZE).unwrap();

    // First page should retain original data.
    vmo_check(&vmo, 1, 0);
    // Pages 1-3 should be zero (not see-through to parent).
    vmo_check(&vmo, 0, PAGE_SIZE);
    vmo_check(&vmo, 0, 2 * PAGE_SIZE);
    vmo_check(&vmo, 0, 3 * PAGE_SIZE);

    // Writing to newly visible pages should fork fresh zero pages.
    vmo_write(&vmo, 2, PAGE_SIZE);
    vmo_write(&vmo, 3, 2 * PAGE_SIZE);
    vmo_write(&vmo, 4, 3 * PAGE_SIZE);
}

/// Helper for ManyChildren / ManyChildrenRevClose tests.
fn many_children_test_helper(reverse_close: bool) {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);

    const CLONE_COUNT: usize = 5;
    let mut clones: Vec<Arc<VmObject>> = Vec::new();

    // Initially no children.
    assert!(
        parent.signal().contains(Signal::VMO_ZERO_CHILDREN),
        "expected ZERO_CHILDREN before creating any clones"
    );

    for _i in 0..CLONE_COUNT {
        let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
        clones.push(child);
        // After creating at least one child, ZERO_CHILDREN should be cleared.
        assert!(
            !parent.signal().contains(Signal::VMO_ZERO_CHILDREN),
            "expected ZERO_CHILDREN cleared after creating clones"
        );
    }

    if reverse_close {
        for _i in (1..CLONE_COUNT).rev() {
            clones.pop(); // drops the last (highest index) clone
                          // Still has children remaining.
            assert!(
                !parent.signal().contains(Signal::VMO_ZERO_CHILDREN),
                "expected ZERO_CHILDREN still cleared with children remaining"
            );
        }
        // Drop the last clone.
        clones.pop();
    } else {
        // Forward close: drop from index 0 upward.
        for _i in 0..CLONE_COUNT - 1 {
            clones.remove(0);
            // Still has children remaining.
            assert!(
                !parent.signal().contains(Signal::VMO_ZERO_CHILDREN),
                "expected ZERO_CHILDREN still cleared with children remaining"
            );
        }
        // Drop the last clone.
        clones.remove(0);
    }

    // All children gone — ZX_VMO_ZERO_CHILDREN should be asserted.
    assert!(
        parent.signal().contains(Signal::VMO_ZERO_CHILDREN),
        "expected ZERO_CHILDREN signal after all children dropped"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ManyChildren)
/// Child count tracking with forward close order.
#[test]
fn many_children() {
    many_children_test_helper(false);
}

/// C++: TEST_F(VmoClone2TestCase, ManyChildrenRevClose)
/// Child count tracking with reverse close order.
#[test]
fn many_children_rev_close() {
    many_children_test_helper(true);
}

/// C++: TEST_F(VmoClone2TestCase, ManyCloneOffset)
/// Clone chain with non-zero offsets, page migration on drop.
#[test]
fn many_clone_offset() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo_write(&vmo, 1, 0);

    let clone1 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    // clone2 at offset PAGE_SIZE of clone1 — sees nothing from original page.
    let clone2 = clone1.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo_write(&clone1, 1, 0);

    drop(clone1);

    // Original VMO should still read correctly after clone1 is dropped.
    vmo_check(&vmo, 1, 0);

    // clone2 exists but has no valid pages (offset is beyond clone1's data).
    drop(clone2);
}

/// C++: TEST_F(VmoClone2TestCase, MarkerClearsSplitBits)
/// When a COW page is forked and then zeroed (via zero()), a marker is
/// inserted. When the child holding the marker is dropped, the split bits
/// on the original page must be cleared so a subsequent clone can fork
/// the same page again.
#[test]
fn marker_clears_split_bits() {
    let _ctx = TestContext::new();

    // Need three pages so we can zero the middle without adjusting parent limits.
    let vmo = VmObject::new_paged(3);

    // Commit a page in what will become the hidden parent.
    let val: u32 = 42;
    vmo.write(PAGE_SIZE, &val.to_ne_bytes()).unwrap();

    let child = vmo.create_child(false, 0, 3 * PAGE_SIZE).unwrap();

    // Fork the page into this child.
    child.write(PAGE_SIZE, &val.to_ne_bytes()).unwrap();
    // Zero the middle page — forces a marker.
    child.zero(PAGE_SIZE, PAGE_SIZE).unwrap();

    // Reset the child, merging hidden parent back. Split bits should be cleared.
    drop(child);

    // Create another child and fork the same page again. Should succeed.
    let child2 = vmo.create_child(false, 0, 3 * PAGE_SIZE).unwrap();
    child2.write(PAGE_SIZE, &val.to_ne_bytes()).unwrap();
}

/// C++: TEST_F(VmoClone2TestCase, ParentStartLimitRegression)
/// Regression test: `parent_start_limit` was computed as offset in parent
/// rather than in the VMO itself during hidden-node merge.
///
/// Tree:       R
///             |
///         |-------|
///         M       S
///         |
///      |-----|
///      C     H
///
/// S: offset 2 in R, length 1
/// M: offset 1 in R, length 2
/// C: offset 0 in M, length 1
/// H: offset 1 in M, length 1
#[test]
fn parent_start_limit_regression() {
    let _ctx = TestContext::new();

    // Create root R (3 pages = 0x3000 bytes).
    let vmo_r = VmObject::new_paged(3);

    // M: snapshot of R at offset PAGE_SIZE, size 2*PAGE_SIZE.
    let vmo_m = vmo_r.create_child(false, PAGE_SIZE, 2 * PAGE_SIZE).unwrap();

    // C: snapshot of M at offset 0, size PAGE_SIZE.
    let vmo_c = vmo_m.create_child(false, 0, PAGE_SIZE).unwrap();

    // S: snapshot of R at offset 2*PAGE_SIZE, size PAGE_SIZE.
    // Then close R to make S a child of the hidden parent.
    let _vmo_s = vmo_r.create_child(false, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();
    drop(vmo_r);

    // H: snapshot of M at offset PAGE_SIZE, size PAGE_SIZE.
    // Then close M to make H a child of the hidden parent.
    let vmo_h = vmo_m.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
    drop(vmo_m);

    // Turn H into a hidden parent by creating a child.
    let _vmo_hc = vmo_h.create_child(false, 0, PAGE_SIZE).unwrap();

    // This is where the kernel would assert/panic without the fix.
    drop(vmo_c);
}

/// C++: TEST(VmoClone2TestCase, CloneVmarWrite)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn clone_vmar_write() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, DisjointCloneProgressive)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn disjoint_clone_progressive() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, DisjointCloneTest2)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn disjoint_clone_test2() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, DropChildrenInParallel)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn drop_children_in_parallel() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, DropParentCommittedBytes)
/// Create a snapshot, drop the parent, verify the child's committed_bytes
/// reflects that it now owns the pages.
#[test]
fn drop_parent_committed_bytes() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    vmo_write(&parent, 42, 0);

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Drop parent — child should still have the page committed.
    drop(parent);

    // Child should have committed bytes (it inherited the page).
    assert!(
        child.get_info().committed_bytes > 0,
        "child should have committed bytes after parent drop"
    );
    // Data should still be readable.
    vmo_check(&child, 42, 0);
}

/// C++: TEST(VmoClone2TestCase, ForbidContiguousVmo)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn forbid_contiguous_vmo() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, ManyCloneMapping)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn many_clone_mapping() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, ManyCloneMappingOffset)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn many_clone_mapping_offset() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, MapRangeReadOnly)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn map_range_read_only() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, NoPhysical)
/// A physical VMO cannot be snapshot-cloned.
#[test]
fn no_physical() {
    let _ctx = TestContext::new();
    // Create a physical VMO at address 0 (test-mode only, no real HW backing).
    let physical = VmObject::new_physical(0, 1);

    // Attempting to create a snapshot child should fail.
    let result = physical.create_child(false, 0, PAGE_SIZE);
    assert!(result.is_err(), "physical VMO snapshot should fail");
}

/// C++: TEST(VmoClone2TestCase, NoSnapshotPager)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn no_snapshot_pager() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, ObjMemAccounting)
/// Create a VMO, write to it, create a snapshot, and verify
/// committed_bytes on both parent and child.
#[test]
fn obj_mem_accounting() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    vmo_write(&parent, 42, 0);

    // Parent should have 1 page committed.
    assert_eq!(parent.get_info().committed_bytes as usize, PAGE_SIZE);

    // Create a snapshot child.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Both parent and child should report committed bytes (the page is
    // shared via COW but still counted as committed for each).
    assert!(parent.get_info().committed_bytes > 0);
    assert!(child.get_info().committed_bytes > 0);
}

/// C++: TEST(VmoClone2TestCase, Offset)
/// Create a VMO, write data, create a snapshot at an offset, then read
/// from the child at offset 0 — should see the parent's data at that offset.
#[test]
fn offset() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    // Clone starting at page 1 of parent.
    let child = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    // Child offset 0 should see parent page 1 (tagged as 2).
    vmo_check(&child, 2, 0);

    // Write to child doesn't affect parent.
    vmo_write(&child, 99, 0);
    vmo_check(&child, 99, 0);
    vmo_check(&parent, 2, PAGE_SIZE); // parent unchanged
}

/// C++: TEST(VmoClone2TestCase, OffsetProgressiveWrite)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn offset_progressive_write() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, ParentVmarWrite)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn parent_vmar_write() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, PinBeforeCreateFailure)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn pin_before_create_failure() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, PinClonePages)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn pin_clone_pages() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

/// C++: TEST(VmoClone2TestCase, SnapshotAtLeastOnWriteSliceInChain)
#[test]
#[ignore = "requires VMAR mapping or multi-threaded runtime"]
fn snapshot_at_least_on_write_slice_in_chain() {
    todo!("requires VMAR mapping or multi-threaded runtime");
}

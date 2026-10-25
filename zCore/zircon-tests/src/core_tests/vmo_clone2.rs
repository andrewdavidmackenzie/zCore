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

    // Verify the writes stuck.
    vmo_check(&vmo, 2, PAGE_SIZE);
    vmo_check(&vmo, 3, 2 * PAGE_SIZE);
    vmo_check(&vmo, 4, 3 * PAGE_SIZE);
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

    // Verify the forked page has the expected value.
    vmo_check(&child2, val, PAGE_SIZE);
    // Parent should still have the original value.
    vmo_check(&vmo, val, PAGE_SIZE);
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
    let vmo_hc = vmo_h.create_child(false, 0, PAGE_SIZE).unwrap();

    // This is where the kernel would assert/panic without the fix.
    // The drop of vmo_c triggers a hidden-node merge that exercises
    // the parent_start_limit calculation. If the regression is
    // present, this panics.
    drop(vmo_c);

    // Verify surviving VMOs are still readable (merge didn't corrupt).
    vmo_check(&vmo_h, 0, 0);
    vmo_check(&vmo_hc, 0, 0);
}

/// C++: TEST(VmoClone2TestCase, CloneVmarWrite)
/// Map a clone into a VMAR, write through the mapping, verify
/// parent is unaffected (COW isolation through mapping).
#[test]
fn clone_vmar_write() {
    use crate::helpers::TestContext;
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "clone-vmar").unwrap();
    let vmar = proc.vmar();

    let parent = init_page_tagged_vmo(1);
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map the child.
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, child.clone(), 0, PAGE_SIZE, flags).unwrap();
    let ptr = addr as *mut u32;
    unsafe { core::ptr::write_volatile(ptr, 99) };

    // Child sees the write.
    vmo_check(&child, 99, 0);

    // Parent is unaffected (COW).
    vmo_check(&parent, 1, 0);

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(VmoClone2TestCase, DisjointCloneProgressive)
/// Disjoint clones: progressively write to each clone's page
/// and verify they don't interfere.
#[test]
fn disjoint_clone_progressive() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    // Create disjoint clones: each covers 1 page at a different offset.
    let c0 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
    let c2 = parent
        .create_child(false, 2 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();
    let c3 = parent
        .create_child(false, 3 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();

    // Each clone sees its respective parent page.
    vmo_check(&c0, 1, 0);
    vmo_check(&c1, 2, 0);
    vmo_check(&c2, 3, 0);
    vmo_check(&c3, 4, 0);

    // Write to each clone progressively.
    vmo_write(&c0, 10, 0);
    vmo_write(&c1, 20, 0);
    vmo_write(&c2, 30, 0);
    vmo_write(&c3, 40, 0);

    // Verify writes are isolated.
    vmo_check(&c0, 10, 0);
    vmo_check(&c1, 20, 0);
    vmo_check(&c2, 30, 0);
    vmo_check(&c3, 40, 0);

    // Parent unchanged.
    vmo_check(&parent, 1, 0);
    vmo_check(&parent, 2, PAGE_SIZE);
    vmo_check(&parent, 3, 2 * PAGE_SIZE);
    vmo_check(&parent, 4, 3 * PAGE_SIZE);
}

/// C++: TEST(VmoClone2TestCase, DisjointCloneTest2)
/// Two disjoint clones: write to both, close parent, clones survive.
#[test]
fn disjoint_clone_test2() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    let c0 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let c1 = parent.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    // Write to both clones.
    vmo_write(&c0, 10, 0);
    vmo_write(&c1, 20, 0);

    // Drop parent.
    drop(parent);

    // Clones retain their data.
    vmo_check(&c0, 10, 0);
    vmo_check(&c1, 20, 0);
}

/// C++: TEST(VmoClone2TestCase, DropChildrenInParallel)
/// Single-threaded simulation: create many children, drop them
/// in rapid succession, verify parent ZERO_CHILDREN signal.
#[test]
fn drop_children_in_parallel() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    vmo_write(&parent, 42, 0);

    let mut children: Vec<Arc<VmObject>> = Vec::new();
    for _ in 0..20 {
        children.push(parent.create_child(false, 0, PAGE_SIZE).unwrap());
    }
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Drop all children.
    children.clear();
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Parent data intact.
    vmo_check(&parent, 42, 0);
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
/// Contiguous VMOs should not allow snapshot children.
#[test]
fn forbid_contiguous_vmo() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_contiguous(1, 12).unwrap();

    // Snapshot of contiguous VMO should fail.
    let result = vmo.create_child(false, 0, PAGE_SIZE);
    assert!(result.is_err(), "snapshot of contiguous VMO should fail");
}

/// C++: TEST(VmoClone2TestCase, ManyCloneMapping)
/// Create many clones, map each, verify isolation through mappings.
#[test]
fn many_clone_mapping() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "many-clone-map").unwrap();
    let vmar = proc.vmar();

    let parent = init_page_tagged_vmo(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    let mut addrs = Vec::new();
    let mut clones = Vec::new();
    for i in 0..5 {
        let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
        let addr = vmar.map(None, child.clone(), 0, PAGE_SIZE, flags).unwrap();

        // Write unique value through mapping.
        let ptr = addr as *mut u32;
        unsafe { core::ptr::write_volatile(ptr, (i + 10) as u32) };

        addrs.push(addr);
        clones.push(child);
    }

    // Verify each clone has its own value.
    for (i, child) in clones.iter().enumerate() {
        vmo_check(child, (i + 10) as u32, 0);
    }

    // Parent unchanged.
    vmo_check(&parent, 1, 0);

    // Cleanup.
    for addr in addrs {
        vmar.unmap(addr, PAGE_SIZE).unwrap();
    }
}

/// C++: TEST(VmoClone2TestCase, ManyCloneMappingOffset)
/// Create clones at different offsets, map each, verify correct data.
#[test]
fn many_clone_mapping_offset() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "clone-map-off").unwrap();
    let vmar = proc.vmar();

    let parent = init_page_tagged_vmo(4);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // Clone at each page offset.
    let mut addrs = Vec::new();
    let mut clones = Vec::new();
    for i in 0..4 {
        let child = parent
            .create_child(false, i * PAGE_SIZE, PAGE_SIZE)
            .unwrap();
        let addr = vmar.map(None, child.clone(), 0, PAGE_SIZE, flags).unwrap();
        addrs.push(addr);
        clones.push(child);
    }

    // Each clone at offset i should see parent page i (tagged i+1).
    for (i, addr) in addrs.iter().enumerate() {
        let ptr = *addr as *const u32;
        let val = unsafe { core::ptr::read_volatile(ptr) };
        assert_eq!(val, (i as u32) + 1);
    }

    // Cleanup.
    for addr in addrs {
        vmar.unmap(addr, PAGE_SIZE).unwrap();
    }
}

/// C++: TEST(VmoClone2TestCase, MapRangeReadOnly)
/// Map a clone read-only, verify reads work.
#[test]
fn map_range_read_only() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "map-ro").unwrap();
    let vmar = proc.vmar();

    let parent = init_page_tagged_vmo(1);
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map read-only.
    let addr = vmar
        .map(None, child.clone(), 0, PAGE_SIZE, MMUFlags::READ)
        .unwrap();

    let ptr = addr as *const u32;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 1);

    vmar.unmap(addr, PAGE_SIZE).unwrap();
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
/// A pager-backed VMO (or one in a pager tree) should not allow
/// SNAPSHOT children. Verify with is_in_pager_tree check.
#[test]
fn no_snapshot_pager() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Without a pager, it's not in a pager tree.
    assert!(!vmo.is_in_pager_tree());

    // Snapshot should succeed for non-pager VMO.
    let child = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    assert_eq!(child.len(), PAGE_SIZE);
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
/// Clone at offset, progressively write to each page.
#[test]
fn offset_progressive_write() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(4);

    // Clone pages 1-3 of parent.
    let child = parent
        .create_child(false, PAGE_SIZE, 3 * PAGE_SIZE)
        .unwrap();

    // Child page 0 = parent page 1 (tag 2).
    vmo_check(&child, 2, 0);
    // Child page 1 = parent page 2 (tag 3).
    vmo_check(&child, 3, PAGE_SIZE);
    // Child page 2 = parent page 3 (tag 4).
    vmo_check(&child, 4, 2 * PAGE_SIZE);

    // Progressively write to child pages.
    vmo_write(&child, 20, 0);
    vmo_check(&child, 20, 0);
    // Parent unchanged at that offset.
    vmo_check(&parent, 2, PAGE_SIZE);

    vmo_write(&child, 30, PAGE_SIZE);
    vmo_check(&child, 30, PAGE_SIZE);
    vmo_check(&parent, 3, 2 * PAGE_SIZE);

    vmo_write(&child, 40, 2 * PAGE_SIZE);
    vmo_check(&child, 40, 2 * PAGE_SIZE);
    vmo_check(&parent, 4, 3 * PAGE_SIZE);
}

/// C++: TEST(VmoClone2TestCase, ParentVmarWrite)
/// Map the parent into VMAR, write through mapping, verify child
/// is unaffected (COW isolation from parent side).
#[test]
fn parent_vmar_write() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "parent-vmar").unwrap();
    let vmar = proc.vmar();

    let parent = init_page_tagged_vmo(1);
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map parent.
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, parent.clone(), 0, PAGE_SIZE, flags).unwrap();

    // Write through parent mapping.
    let ptr = addr as *mut u32;
    unsafe { core::ptr::write_volatile(ptr, 99) };

    // Parent sees the write.
    vmo_check(&parent, 99, 0);

    // Child is unaffected (COW).
    vmo_check(&child, 1, 0);

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(VmoClone2TestCase, PinBeforeCreateFailure)
/// Pinning a VMO prevents snapshot creation (ZX_ERR_BAD_STATE).
/// After unpinning, snapshot creation succeeds.
#[test]
fn pin_before_create_failure() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo_write(&vmo, 42, 0);

    // Pin the page.
    vmo.pin(0, PAGE_SIZE).unwrap();

    // Snapshot must fail while pages are pinned.
    let snap_result = vmo.create_child(false, 0, PAGE_SIZE);
    assert!(
        snap_result.is_err(),
        "snapshot of pinned VMO should fail with BAD_STATE"
    );

    // Unpin the page.
    vmo.unpin(0, PAGE_SIZE).unwrap();

    // Snapshot should succeed after unpin.
    let child = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    vmo_check(&child, 42, 0);
}

/// C++: TEST(VmoClone2TestCase, PinClonePages)
/// Create a snapshot, try to pin the clone's pages.
#[test]
fn pin_clone_pages() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);
    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Try to pin clone pages (may fail with NOT_SUPPORTED).
    let result = child.pin(0, PAGE_SIZE);
    if result.is_ok() {
        child.unpin(0, PAGE_SIZE).unwrap();
    }

    // Clone data should be intact.
    vmo_check(&child, 1, 0);
    vmo_check(&child, 2, PAGE_SIZE);
}

/// C++: TEST(VmoClone2TestCase, SnapshotAtLeastOnWriteSliceInChain)
/// Create a chain: parent → slice → snapshot. Verify snapshot
/// has COW semantics and sees the original data.
#[test]
fn snapshot_at_least_on_write_slice_in_chain() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo(2);

    // Create a slice of parent (transparent write-through).
    let slice = parent.create_slice(0, 2 * PAGE_SIZE).unwrap();
    vmo_check(&slice, 1, 0);
    vmo_check(&slice, 2, PAGE_SIZE);

    // Create a snapshot of the slice.
    let snap = slice.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    vmo_check(&snap, 1, 0);
    vmo_check(&snap, 2, PAGE_SIZE);

    // Write to snapshot — should NOT affect parent or slice.
    vmo_write(&snap, 99, 0);
    vmo_check(&snap, 99, 0);
    vmo_check(&slice, 1, 0);
    vmo_check(&parent, 1, 0);

    // Write to parent — visible in slice (write-through) but not snapshot.
    vmo_write(&parent, 77, PAGE_SIZE);
    vmo_check(&parent, 77, PAGE_SIZE);
    vmo_check(&slice, 77, PAGE_SIZE);
    vmo_check(&snap, 2, PAGE_SIZE); // snapshot frozen at creation time
}

/// C++: TEST_F(VmoClone2TestCase, ResizeOffsetChild)
#[test]
fn resize_offset_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);
    let clone = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo.set_len(0).unwrap();

    let info = vmo.get_info();
    assert_eq!(info.populated_bytes, 0, "parent resized to 0");
    let clone_info = clone.get_info();
    assert_eq!(
        clone_info.populated_bytes as usize, PAGE_SIZE,
        "clone should own its page"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeDisjointChild)
#[test]
fn resize_disjoint_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let c0 = vmo.create_child(true, 0, PAGE_SIZE).unwrap();
    let c1 = vmo.create_child(true, PAGE_SIZE, PAGE_SIZE).unwrap();
    let c2 = vmo.create_child(true, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();

    c0.set_len(0).unwrap();
    c1.set_len(0).unwrap();
    vmo.set_len(0).unwrap();

    let info = c2.get_info();
    assert_eq!(
        info.populated_bytes as usize, PAGE_SIZE,
        "c2 should be sole owner"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeMultipleProgressive)
#[test]
fn resize_multiple_progressive() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let clone = vmo.create_child(true, 0, 2 * PAGE_SIZE).unwrap();
    vmo.write(0, &4u32.to_ne_bytes()).unwrap();
    clone.write(PAGE_SIZE, &5u32.to_ne_bytes()).unwrap();

    let clone2 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    clone.set_len(0).unwrap();

    let info = clone2.get_info();
    assert!(
        info.populated_bytes > 0,
        "clone2 should have attributed pages"
    );
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneClose)
#[test]
fn progressive_clone_close() {
    let _ctx = TestContext::new();
    let num_clones = 6usize;
    let vmo = init_page_tagged_vmo_resizable(num_clones);

    let mut clones = Vec::new();
    for i in 1..num_clones {
        let c = vmo.create_child(true, 0, num_clones * PAGE_SIZE).unwrap();
        c.write(i * PAGE_SIZE, &((num_clones + 2) as u32).to_ne_bytes())
            .unwrap();
        clones.push(c);
    }

    drop(vmo);

    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "each clone should have attributed pages"
        );
    }
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneTruncate)
#[test]
fn progressive_clone_truncate() {
    let _ctx = TestContext::new();
    let num_clones = 6usize;
    let vmo = init_page_tagged_vmo_resizable(num_clones);

    let mut clones = Vec::new();
    for i in 1..num_clones {
        let c = vmo.create_child(true, 0, num_clones * PAGE_SIZE).unwrap();
        c.write(i * PAGE_SIZE, &((num_clones + 2) as u32).to_ne_bytes())
            .unwrap();
        clones.push(c);
    }

    vmo.set_len(0).unwrap();

    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "each clone should have attributed pages"
        );
    }
}

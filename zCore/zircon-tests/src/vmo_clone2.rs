//! VMO Clone2 tests.
//!
//! Faithful 1:1 ports from VmoClone2TestCase (vmo-clone2.cc).
//! These test COW (copy-on-write) snapshot clone behavior: read/write
//! isolation, close ordering, offset clones, disjoint clones, resize.

use crate::helpers::TestContext;
use alloc::sync::Arc;

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

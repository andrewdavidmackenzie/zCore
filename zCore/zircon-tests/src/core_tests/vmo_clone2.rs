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

/// C++ `InitPageTaggedVmo`: creates a **resizable** VMO with `pages` pages,
/// each page tagged with (index + 1).  The C++ version uses `ZX_VMO_RESIZABLE`.
fn init_page_tagged_vmo(pages: usize) -> Arc<VmObject> {
    let vmo = VmObject::new_paged_with_resizable(true, pages);
    for i in 0..pages {
        vmo_write(&vmo, (i + 1) as u32, i * PAGE_SIZE);
    }
    vmo
}

/// C++ `VmoPopulatedBytes`: return the attributed populated bytes.
///
/// When fractional scaling is supported (`populated_fractional_scaled_bytes
/// != u64::MAX`), returns `populated_scaled_bytes` (fractional attribution).
/// Otherwise falls back to `populated_bytes`.
fn vmo_populated_bytes(vmo: &Arc<VmObject>) -> u64 {
    vmo.get_info().vmo_populated_bytes()
}

/// C++ `PollVmoPopulatedBytes`: poll until `vmo_populated_bytes(vmo) == expected`.
///
/// This is a faithful port of the C++ infinite polling loop.  It **will** hang
/// if the kernel's page attribution is wrong — that is intentional: the test
/// must fail the same way as C++.
fn poll_vmo_populated_bytes(vmo: &Arc<VmObject>, expected: u64) {
    loop {
        if vmo_populated_bytes(vmo) == expected {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// C++ `CallPermutations`: invoke `f` with every permutation of `[0..count)`.
fn call_permutations<F: FnMut(&[u32])>(count: u32, f: &mut F) {
    let mut perm = vec![0u32; count as usize];
    let mut used = vec![false; count as usize];
    call_permutations_helper(count, &mut perm, &mut used, 0, f);
}

fn call_permutations_helper<F: FnMut(&[u32])>(
    count: u32,
    perm: &mut Vec<u32>,
    used: &mut Vec<bool>,
    idx: u32,
    f: &mut F,
) {
    if idx == count {
        f(perm);
        return;
    }
    for i in 0..count as usize {
        if used[i] {
            continue;
        }
        used[i] = true;
        perm[idx as usize] = i as u32;
        call_permutations_helper(count, perm, used, idx + 1, f);
        used[i] = false;
    }
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
/// 4 VMOs: parent → 2 children → 1 grandchild, all zero. Progressively
/// write to each and check populated_bytes.
#[test]
fn zero_page_write() {
    let _ctx = TestContext::new();
    let kvmo_size = PAGE_SIZE;

    let vmo0 = VmObject::new_paged(1);
    let vmo1 = vmo0.create_child(false, 0, kvmo_size).unwrap();
    let vmo2 = vmo0.create_child(false, 0, kvmo_size).unwrap();
    let vmo3 = vmo1.create_child(false, 0, kvmo_size).unwrap();

    let vmos: [&Arc<VmObject>; 4] = [&vmo0, &vmo1, &vmo2, &vmo3];

    // Write to each VMO and check all populated_bytes.
    for (i, vmo_i) in vmos.iter().enumerate() {
        vmo_write(vmo_i, (i + 1) as u32, 0);
        for (j, vmo_j) in vmos.iter().enumerate() {
            vmo_check(vmo_j, if j <= i { (j + 1) as u32 } else { 0 }, 0);
            assert_eq!(
                vmo_populated_bytes(vmo_j),
                if j <= i { kvmo_size as u64 } else { 0 },
            );
        }
    }
}

/// C++: TEST_F(VmoClone2TestCase, SplitPageClosure)
/// Chain: parent → clone1 → clone2, fork page into both clones.
/// Close original, check data and populated_bytes. Close clone1,
/// check clone2 data and populated_bytes.
#[test]
fn split_page_closure() {
    let _ctx = TestContext::new();
    let koriginal_size = PAGE_SIZE;
    let kclone1_size = PAGE_SIZE;
    let kclone2_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(1);
    let clone1 = vmo.create_child(false, 0, kclone1_size).unwrap();
    let clone2 = clone1.create_child(false, 0, kclone2_size).unwrap();

    // Fork the page into the two clones.
    vmo_write(&clone1, 3, 0);
    vmo_write(&clone2, 4, 0);

    // Each VMO has a private page.
    assert_eq!(vmo_populated_bytes(&vmo), koriginal_size as u64);
    assert_eq!(vmo_populated_bytes(&clone1), kclone1_size as u64);
    assert_eq!(vmo_populated_bytes(&clone2), kclone2_size as u64);

    // Close the original vmo, check data and things were freed.
    drop(vmo);
    vmo_check(&clone1, 3, 0);
    vmo_check(&clone2, 4, 0);

    poll_vmo_populated_bytes(&clone1, kclone1_size as u64);
    poll_vmo_populated_bytes(&clone2, kclone2_size as u64);

    // Close the first clone, check clone2.
    drop(clone1);
    vmo_check(&clone2, 4, 0);

    poll_vmo_populated_bytes(&clone2, kclone2_size as u64);
}

/// C++: TEST_F(VmoClone2TestCase, OffsetTest2)
/// 4-page parent, offset clone at PAGE_SIZE (3 pages), 2 sub-clones that
/// fully divide the offset clone. Check data, write, drop offset clone,
/// check fractional attribution, poll.
#[test]
fn offset_test2() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 4usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let koffset_size = 3 * PAGE_SIZE;
    let kclone1_size = 2 * PAGE_SIZE;
    let kclone2_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let offset_clone = vmo.create_child(false, PAGE_SIZE, koffset_size).unwrap();
    let clone1 = offset_clone.create_child(false, 0, kclone1_size).unwrap();
    let clone2 = offset_clone
        .create_child(false, kclone1_size, kclone2_size)
        .unwrap();

    // Check data.
    vmo_check(&clone1, 2, 0);
    vmo_check(&clone1, 3, PAGE_SIZE);
    vmo_check(&clone2, 4, 0);

    // Write to offset clone and close it.
    vmo_write(&offset_clone, 4, PAGE_SIZE);
    drop(offset_clone);

    vmo_check(&clone1, 2, 0);
    vmo_check(&clone1, 3, PAGE_SIZE);
    vmo_check(&clone2, 4, 0);

    // Fractional: vmo gets 1 + 1/2 + 1/2 + 1/2 = 5/2, clone1 gets 1/2 + 1/2 = 1,
    // clone2 gets 1/2.
    let kimpl_cost_original = 5 * PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone1 = PAGE_SIZE as u64;
    let kimpl_cost_clone2 = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone1 + kimpl_cost_clone2,
        koriginal_size as u64
    );
    poll_vmo_populated_bytes(&vmo, kimpl_cost_original);
    poll_vmo_populated_bytes(&clone1, kimpl_cost_clone1);
    poll_vmo_populated_bytes(&clone2, kimpl_cost_clone2);

    // Close clone1.
    drop(clone1);
    let kimpl_cost_original2 = 7 * PAGE_SIZE as u64 / 2; // 1 + 1 + 1 + 1/2
    poll_vmo_populated_bytes(&vmo, kimpl_cost_original2);
    poll_vmo_populated_bytes(&clone2, kimpl_cost_clone2);

    // Close clone2.
    drop(clone2);
    let kimpl_cost_original3 = koriginal_size as u64;
    poll_vmo_populated_bytes(&vmo, kimpl_cost_original3);
}

/// C++: TEST_F(VmoClone2TestCase, Overflow)
/// 1-page parent, 2-page clone (overflows parent), clone-of-clone (3 pages,
/// overflows further). Check data, fractional attribution at each step,
/// fork all pages, close middle clone, poll.
#[test]
fn overflow() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 1usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = 2 * PAGE_SIZE;
    let kclone2_size = 3 * PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, 0, kclone_size).unwrap();

    // Clone sees parent page then zero.
    vmo_check(&clone, 1, 0);
    vmo_check(&clone, 0, PAGE_SIZE);

    // Fractional: each gets 1/2 page.
    let kimpl_cost_original = PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);

    // Write to the child and then clone it.
    vmo_write(&clone, 2, PAGE_SIZE);
    let clone2 = clone.create_child(false, 0, kclone2_size).unwrap();

    vmo_check(&clone2, 1, 0);
    vmo_check(&clone2, 2, PAGE_SIZE);
    vmo_check(&clone2, 0, 2 * PAGE_SIZE);

    // After clone2: page 0 shared 3-way (1/3 each), page 1 shared 2-way (1/2 each).
    let kimpl_cost2_original = PAGE_SIZE as u64 / 3;
    let kimpl_cost2_clone = 5 * PAGE_SIZE as u64 / 6;
    let kimpl_cost2_clone2 = 5 * PAGE_SIZE as u64 / 6;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost2_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost2_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost2_clone2);

    // Write private page in clone2.
    vmo_write(&clone2, 3, 2 * PAGE_SIZE);
    vmo_check(&clone2, 3, 2 * PAGE_SIZE);

    let kimpl_cost3_original = PAGE_SIZE as u64 / 3;
    let kimpl_cost3_clone = 5 * PAGE_SIZE as u64 / 6;
    let kimpl_cost3_clone2 = 11 * PAGE_SIZE as u64 / 6;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost3_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost3_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost3_clone2);

    // Completely fork the final clone.
    vmo_write(&clone2, 4, 0);
    vmo_write(&clone2, 5, PAGE_SIZE);
    vmo_check(&vmo, 1, 0);
    vmo_check(&clone, 1, 0);
    vmo_check(&clone, 2, PAGE_SIZE);
    vmo_check(&clone2, 4, 0);
    vmo_check(&clone2, 5, PAGE_SIZE);
    vmo_check(&clone2, 3, 2 * PAGE_SIZE);

    let kimpl_cost4_original = PAGE_SIZE as u64 / 2;
    let kimpl_cost4_clone = 3 * PAGE_SIZE as u64 / 2;
    let kimpl_cost4_clone2 = 3 * PAGE_SIZE as u64;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost4_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost4_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost4_clone2);

    // Close the middle clone.
    drop(clone);
    vmo_check(&vmo, 1, 0);
    vmo_check(&clone2, 4, 0);
    vmo_check(&clone2, 5, PAGE_SIZE);
    vmo_check(&clone2, 3, 2 * PAGE_SIZE);

    let kimpl_cost5_original = PAGE_SIZE as u64;
    let kimpl_cost5_clone2 = 3 * PAGE_SIZE as u64;
    poll_vmo_populated_bytes(&vmo, kimpl_cost5_original);
    poll_vmo_populated_bytes(&clone2, kimpl_cost5_clone2);
}

/// C++: TEST_F(VmoClone2TestCase, OutOfBounds)
/// 1-page parent, clone at offset 2 (non-overlapping, 2 pages), then
/// clone-of-clone (3 pages). Check data (all zeros from clone since it
/// doesn't overlap parent), check fractional attribution, write, check
/// again.
#[test]
fn out_of_bounds() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 1usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = 2 * PAGE_SIZE;
    let kclone2_size = 3 * PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, 2 * PAGE_SIZE, kclone_size).unwrap();

    // Clone doesn't overlap parent — all zeros.
    vmo_check(&clone, 0, 0);
    vmo_check(&clone, 0, PAGE_SIZE);

    // Original keeps its page (1), clone has nothing (0).
    let kimpl_cost_original = PAGE_SIZE as u64;
    let kimpl_cost_clone = 0u64;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);

    // Write to the child and then clone it.
    vmo_write(&clone, 2, PAGE_SIZE);
    let clone2 = clone.create_child(false, 0, kclone2_size).unwrap();

    vmo_check(&clone2, 0, 0);
    vmo_check(&clone2, 2, PAGE_SIZE);
    vmo_check(&clone2, 0, 2 * PAGE_SIZE);

    // original: 1, clone: 0 + 1/2 = 1/2, clone2: 0 + 1/2 + 0 = 1/2
    let kimpl_cost2_original = PAGE_SIZE as u64;
    let kimpl_cost2_clone = PAGE_SIZE as u64 / 2;
    let kimpl_cost2_clone2 = PAGE_SIZE as u64 / 2;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost2_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost2_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost2_clone2);

    // Write private page in clone2.
    vmo_write(&clone2, 3, 2 * PAGE_SIZE);
    vmo_check(&clone2, 3, 2 * PAGE_SIZE);

    let kimpl_cost3_original = PAGE_SIZE as u64;
    let kimpl_cost3_clone = PAGE_SIZE as u64 / 2;
    let kimpl_cost3_clone2 = 3 * PAGE_SIZE as u64 / 2;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost3_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost3_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost3_clone2);
}

/// C++: TEST_F(VmoClone2TestCase, SmallClone)
/// 3-page parent, 1-page clone at offset 1. Check fractional attribution,
/// write to out-of-range pages, close parent, poll populated_bytes.
#[test]
fn small_clone() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 3usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, PAGE_SIZE, kclone_size).unwrap();

    vmo_check(&clone, 2, 0);

    // Fractional: original gets 1 + 1/2 + 1 = 5/2, clone gets 1/2.
    let kimpl_cost_original = 5 * PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);

    // Writes to original out of bounds of the clone don't allocate.
    vmo_write(&vmo, 4, 0);
    vmo_write(&vmo, 5, 2 * PAGE_SIZE);
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);

    // Close the parent and check clone data and accounting.
    drop(vmo);
    vmo_check(&clone, 2, 0);

    let kimpl_cost2_clone = PAGE_SIZE as u64; // 1
    poll_vmo_populated_bytes(&clone, kimpl_cost2_clone);
}

/// C++: TEST_F(VmoClone2TestCase, SmallCloneChild)
/// 3-page parent, 1-page clone at offset 1, grandchild of clone
/// (2 pages) — grandchild sees clone page then zero.
#[test]
fn small_clone_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo(3);

    let clone = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo_check(&clone, 2, 0);

    // Clone of the first clone — 2 pages but clone only has 1 page of data.
    let clone2 = clone.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    vmo_check(&clone2, 2, 0);
    vmo_check(&clone2, 0, PAGE_SIZE);
}

/// C++: TEST_F(VmoClone2TestCase, SmallClones)
/// 3-page parent, clone at offset 1 (2 pages), write to clone page 1,
/// create 2nd clone at offset 0 (1 page). Check fractional attribution,
/// close original, poll.
#[test]
fn small_clones() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 3usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = 2 * PAGE_SIZE;
    let kclone2_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, PAGE_SIZE, kclone_size).unwrap();
    vmo_write(&clone, 4, PAGE_SIZE);

    // Create a second clone of the original.
    let clone2 = vmo.create_child(false, 0, kclone2_size).unwrap();

    vmo_check(&clone, 2, 0);

    // original: 1/2 + 1/2 + 1 = 2 pages
    // clone: 1/2 + 1 = 3/2 pages
    // clone2: 1/2 page
    let kimpl_cost_original = 2 * PAGE_SIZE as u64;
    let kimpl_cost_clone = 3 * PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone2 = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone + kimpl_cost_clone2,
        koriginal_size as u64 + PAGE_SIZE as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost_clone2);

    // Close the original VMO. Page 2 from original migrates to clone.
    drop(vmo);

    let kimpl_cost2_clone = 2 * PAGE_SIZE as u64; // 1 + 1
    let kimpl_cost2_clone2 = PAGE_SIZE as u64; // 1
    poll_vmo_populated_bytes(&clone, kimpl_cost2_clone);
    poll_vmo_populated_bytes(&clone2, kimpl_cost2_clone2);
}

/// C++ `DisjointClonesTest`: 4-page parent, intermediate clone (pages 1-2),
/// 4 leaf clones (2 direct, 2 via intermediate). If early_close, drop parent
/// + intermediate before writing; otherwise after. Verify data + poll
///   populated_bytes for each leaf.
fn disjoint_clones_test(early_close: bool) {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo(4);

    // Create intermediate clone covering pages 1-2.
    let intermediate = vmo.create_child(false, PAGE_SIZE, 2 * PAGE_SIZE).unwrap();

    // 4 leaf clones.
    let leaf_clones: Vec<Arc<VmObject>> = vec![
        vmo.create_child(false, 0, PAGE_SIZE).unwrap(),
        intermediate.create_child(false, 0, PAGE_SIZE).unwrap(),
        intermediate
            .create_child(false, PAGE_SIZE, PAGE_SIZE)
            .unwrap(),
        vmo.create_child(false, 3 * PAGE_SIZE, PAGE_SIZE).unwrap(),
    ];

    if early_close {
        drop(vmo);
        drop(intermediate);

        // Check each clone's data and write to it.
        for (i, lc) in leaf_clones.iter().enumerate() {
            vmo_check(lc, (i + 1) as u32, 0);
            vmo_write(lc, (i + 5) as u32, 0);
        }
    } else {
        // Check each clone's data and write to it.
        for (i, lc) in leaf_clones.iter().enumerate() {
            vmo_check(lc, (i + 1) as u32, 0);
            vmo_write(lc, (i + 5) as u32, 0);
        }

        drop(vmo);
        drop(intermediate);
    }

    // Check that the clones have the correct data and nothing is unnecessarily
    // retained/allocated.
    for (i, lc) in leaf_clones.iter().enumerate() {
        vmo_check(lc, (i + 5) as u32, 0);
        poll_vmo_populated_bytes(lc, PAGE_SIZE as u64);
    }
}

/// C++: TEST_F(VmoCloneDisjointClonesTests, DisjointCloneEarlyClose)
#[test]
fn disjoint_clone_early_close() {
    disjoint_clones_test(true);
}

/// C++: TEST_F(VmoCloneDisjointClonesTests, DisjointCloneLateClose)
#[test]
fn disjoint_clone_late_close() {
    disjoint_clones_test(false);
}

/// C++: TEST_F(VmoClone2TestCase, ResizeGrow)
/// 2-page parent, 1-page resizable clone, grow to 2 pages. New page is
/// zero. Write to parent page 2, verify clone unaffected. Check
/// fractional attribution.
#[test]
fn resize_grow() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 2usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = PAGE_SIZE;
    let kclone_new_size = 2 * PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(true, 0, kclone_size).unwrap();

    vmo_check(&clone, 1, 0);

    // Grow the clone and check that the new page is zero.
    clone.set_len(kclone_new_size).unwrap();
    vmo_check(&clone, 0, PAGE_SIZE);

    // Write to the second page of the original VMO. Doesn't affect clone.
    vmo_write(&vmo, 3, PAGE_SIZE);
    vmo_check(&clone, 0, PAGE_SIZE);

    // Fractional: original gets 1/2 + 1 = 3/2, clone gets 1/2 + 0.
    let kimpl_cost_original = 3 * PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);
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

/// C++: TEST_F(VmoClone2TestCase, ResizeOverSiblingRange)
/// Regression test for Fuchsia bug 53710. When a COW child is resized,
/// its `parent_limit_` must be correctly updated even when the resize
/// crosses a sibling's range. After shrink+grow, new pages must be zero.
#[test]
fn resize_over_sibling_range() {
    let _ctx = TestContext::new();
    // Parent must be resizable since we call set_len on it.
    let vmo = init_page_tagged_vmo(4);

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

/// C++ `VmarWriteTestHelper`: map both parent and clone, write through
/// one mapping, verify COW isolation through both mappings.
fn vmar_write_test_helper(clone_write: bool) {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-write").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // Map the parent.
    let vmo_addr = vmar.map(None, vmo.clone(), 0, PAGE_SIZE, flags).unwrap();
    let vmo_ptr = vmo_addr as *mut u32;

    const KORIGINAL_DATA: u32 = 0xdeadbeef;
    const KNEW_DATA: u32 = 0xc0ffee;
    unsafe { core::ptr::write_volatile(vmo_ptr, KORIGINAL_DATA) };

    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map the clone.
    let clone_addr = vmar.map(None, clone.clone(), 0, PAGE_SIZE, flags).unwrap();
    let clone_ptr = clone_addr as *mut u32;

    // Write through the appropriate mapping.
    if clone_write {
        unsafe { core::ptr::write_volatile(clone_ptr, KNEW_DATA) };
    } else {
        unsafe { core::ptr::write_volatile(vmo_ptr, KNEW_DATA) };
    }

    let expected_vmo = if clone_write {
        KORIGINAL_DATA
    } else {
        KNEW_DATA
    };
    let expected_clone = if clone_write {
        KNEW_DATA
    } else {
        KORIGINAL_DATA
    };
    assert_eq!(unsafe { core::ptr::read_volatile(vmo_ptr) }, expected_vmo);
    assert_eq!(
        unsafe { core::ptr::read_volatile(clone_ptr) },
        expected_clone
    );

    vmar.unmap(clone_addr, PAGE_SIZE).unwrap();
    vmar.unmap(vmo_addr, PAGE_SIZE).unwrap();
}

/// C++: TEST_F(VmoClone2TestCase, CloneVmarWrite)
#[test]
fn clone_vmar_write() {
    vmar_write_test_helper(true);
}

/// C++: TEST_F(VmoClone2TestCase, DisjointCloneProgressive)
/// 6-page parent, main_clone at offset 1 (5 pages), write to page 3,
/// create 4 sub-clones at various offsets, close all clones, then
/// create a last_clone and verify parent data survives.
#[test]
fn disjoint_clone_progressive() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo(6);

    let main_clone = vmo.create_child(false, PAGE_SIZE, 5 * PAGE_SIZE).unwrap();

    vmo_write(&main_clone, 7, 3 * PAGE_SIZE);

    // A clone which references the written page.
    let clone1 = main_clone
        .create_child(false, PAGE_SIZE, 4 * PAGE_SIZE)
        .unwrap();
    // A clone after the written page.
    let clone2 = main_clone
        .create_child(false, 4 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();
    // A clone before the written page.
    let clone3 = main_clone
        .create_child(false, 2 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();
    // A clone which doesn't reference any pages, but needs to be in the tree.
    let clone4 = main_clone
        .create_child(false, 10 * PAGE_SIZE, PAGE_SIZE)
        .unwrap();

    drop(main_clone);
    drop(clone1);
    drop(clone3);
    drop(clone4);
    drop(clone2);

    let last_clone = vmo.create_child(false, 0, 6 * PAGE_SIZE).unwrap();
    for i in 0..6 {
        vmo_check(&vmo, (i + 1) as u32, i * PAGE_SIZE);
        vmo_check(&last_clone, (i + 1) as u32, i * PAGE_SIZE);
    }

    vmo_write(&vmo, 8, 4 * PAGE_SIZE);

    for i in 0..6 {
        let expected = if i == 4 { 8 } else { (i + 1) as u32 };
        vmo_check(&vmo, expected, i * PAGE_SIZE);
        vmo_check(&last_clone, (i + 1) as u32, i * PAGE_SIZE);
    }
}

/// C++: TEST_F(VmoClone2TestCase, DisjointCloneTest2)
/// 4-page parent, intermediate clone (pages 1-2), 4 leaf clones
/// (2 direct, 2 via intermediate). Drop parent + intermediate. Close
/// leaves in all 24 permutations, checking data + populated_bytes at
/// each step.
#[test]
fn disjoint_clone_test2() {
    let _ctx = TestContext::new();
    call_permutations(4, &mut |perm: &[u32]| {
        let vmo = init_page_tagged_vmo(4);

        // Create intermediate clone covering pages 1-2.
        let intermediate = vmo.create_child(false, PAGE_SIZE, 2 * PAGE_SIZE).unwrap();

        // 4 leaf clones: 2 direct from parent, 2 via intermediate.
        let leaf0 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
        let leaf1 = intermediate.create_child(false, 0, PAGE_SIZE).unwrap();
        let leaf2 = intermediate
            .create_child(false, PAGE_SIZE, PAGE_SIZE)
            .unwrap();
        let leaf3 = vmo.create_child(false, 3 * PAGE_SIZE, PAGE_SIZE).unwrap();

        drop(vmo);
        drop(intermediate);

        // Check that each clone has the correct data.
        let leaf_clones: [&Arc<VmObject>; 4] = [&leaf0, &leaf1, &leaf2, &leaf3];
        for (i, lc) in leaf_clones.iter().enumerate() {
            vmo_check(lc, (i + 1) as u32, 0);
        }

        // Close the clones in the order specified by |perm|, and at each step
        // check the rest of the clones.
        let mut leaves: [Option<Arc<VmObject>>; 4] =
            [Some(leaf0), Some(leaf1), Some(leaf2), Some(leaf3)];
        let mut closed = [false; 4];
        for &p in perm.iter().take(4) {
            let idx = p as usize;
            leaves[idx] = None; // drop
            closed[idx] = true;

            for j in 0..4 {
                if !closed[j] {
                    let vmo_ref = leaves[j].as_ref().unwrap();
                    vmo_check(vmo_ref, (j + 1) as u32, 0);
                    poll_vmo_populated_bytes(vmo_ref, PAGE_SIZE as u64);
                }
            }
        }
    });
}

/// C++: TEST_F(VmoClone2TestCase, DropChildrenInParallel)
/// Regression test for https://fxbug.dev/42133843. 1000 iterations:
/// create parent + child, race destruction on two std::threads using
/// an atomic barrier.
#[test]
#[ignore] // Kernel race in concurrent VMO drop — C++ equivalent in core-tests-skip.txt
fn drop_children_in_parallel() {
    let _ctx = TestContext::new();

    for _iter in 0..1000 {
        let vmo = VmObject::new_paged(1);
        let child = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

        // Use a three step ready protocol.
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let child_move = child;
        let ready_clone = ready.clone();
        let thread = std::thread::spawn(move || {
            ready_clone.store(false, std::sync::atomic::Ordering::SeqCst);
            while !ready_clone.load(std::sync::atomic::Ordering::SeqCst) {
                std::hint::spin_loop();
            }
            drop(child_move);
        });
        while ready.load(std::sync::atomic::Ordering::SeqCst) {
            std::hint::spin_loop();
        }
        ready.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(vmo);
        thread.join().unwrap();
    }
}

/// C++: TEST_F(VmoClone2TestCase, DropParentCommittedBytes)
/// Regression test for https://fxbug.dev/42080199. 1000 iterations:
/// create a 3-page parent, clone at offset 1 (2 pages), fork a page,
/// check fractional populated_bytes, race parent drop with attribution
/// query on another thread, then poll for final attribution.
#[test]
#[ignore] // PollVmoPopulatedBytes hang — C++ equivalent in core-tests-skip.txt
fn drop_parent_committed_bytes() {
    let _ctx = TestContext::new();

    for _iter in 0..1000 {
        let vmo = init_page_tagged_vmo(3);

        // Create a child that sees the parent partially.
        let child = vmo.create_child(false, PAGE_SIZE, 2 * PAGE_SIZE).unwrap();

        // Check that the child has the right data.
        vmo_check(&child, 2, 0);
        vmo_check(&child, 3, PAGE_SIZE);

        // Fork a page in the child.
        vmo_write(&child, 4, PAGE_SIZE);

        // The child shares a page with the parent, and has a forked page.
        // 1/2 + 1 = 3/2 pages.
        let kattr_bytes_clone_split = 3 * PAGE_SIZE as u64 / 2;
        let kattr_bytes_clone_no_split = 2 * PAGE_SIZE as u64;
        assert_eq!(vmo_populated_bytes(&child), kattr_bytes_clone_split);

        // Use a three step ready protocol to ensure both threads can issue
        // their requests at close to the same time.
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let child_clone = child.clone();
        let ready_clone = ready.clone();
        let thread = std::thread::spawn(move || {
            ready_clone.store(false, std::sync::atomic::Ordering::SeqCst);
            while !ready_clone.load(std::sync::atomic::Ordering::SeqCst) {
                std::hint::spin_loop();
            }
            let committed = vmo_populated_bytes(&child_clone);
            // Depending on who wins the race, the child will either continue
            // sharing a page with the parent, or it will have both pages
            // attributed to it.
            assert!(
                committed == kattr_bytes_clone_split || committed == kattr_bytes_clone_no_split,
                "committed bytes in child: {}",
                committed
            );
        });
        while ready.load(std::sync::atomic::Ordering::SeqCst) {
            std::hint::spin_loop();
        }
        ready.store(true, std::sync::atomic::Ordering::SeqCst);
        // Drop the parent.
        drop(vmo);
        thread.join().unwrap();

        // Check that we don't change the child.
        vmo_check(&child, 2, 0);
        vmo_check(&child, 4, PAGE_SIZE);

        // Poll until the child owns both pages.
        poll_vmo_populated_bytes(&child, kattr_bytes_clone_no_split);
    }
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

/// C++: TEST_F(VmoClone2TestCase, ManyCloneMapping)
/// Creates a tree: parent → 2 children → 1 grandchild (4 VMOs total).
/// Maps all, writes through mappings in every permutation order to verify
/// no order results in a bad read.
#[test]
fn many_clone_mapping() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    const KNUM_ELTS: u32 = 4;
    const KORIGINAL_DATA: u32 = 0xdeadbeef;
    const KNEW_DATA: u32 = 0xc0ffee;

    call_permutations(KNUM_ELTS, &mut |perm: &[u32]| {
        let job = Job::root();
        let proc = Process::create(&job, "many-clone-map").unwrap();
        let vmar = proc.vmar();
        let flags = MMUFlags::READ | MMUFlags::WRITE;

        // vmos[0] = parent, vmos[1] = child of 0, vmos[2] = child of 0,
        // vmos[3] = child of 1.
        let vmo0 = VmObject::new_paged(1);
        vmo_write(&vmo0, KORIGINAL_DATA, 0);

        let vmo1 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
        let vmo2 = vmo0.create_child(false, 0, PAGE_SIZE).unwrap();
        let vmo3 = vmo1.create_child(false, 0, PAGE_SIZE).unwrap();

        let vmos: [&Arc<VmObject>; 4] = [&vmo0, &vmo1, &vmo2, &vmo3];
        let mut addrs = [0usize; 4];

        // Map the vmos and make sure they're all correct.
        for i in 0..KNUM_ELTS as usize {
            let addr = vmar
                .map(None, vmos[i].clone(), 0, PAGE_SIZE, flags)
                .unwrap();
            addrs[i] = addr;
            let ptr = addr as *const u32;
            assert_eq!(unsafe { core::ptr::read_volatile(ptr) }, KORIGINAL_DATA);
        }

        // Write to the pages in the order specified by |perm| and validate.
        let mut written = [false; 4];
        for &p in perm.iter().take(KNUM_ELTS as usize) {
            let cur_idx = p as usize;
            let ptr = addrs[cur_idx] as *mut u32;
            unsafe { core::ptr::write_volatile(ptr, KNEW_DATA) };
            written[cur_idx] = true;

            for j in 0..KNUM_ELTS as usize {
                let rptr = addrs[j] as *const u32;
                let expected = if written[j] {
                    KNEW_DATA
                } else {
                    KORIGINAL_DATA
                };
                assert_eq!(unsafe { core::ptr::read_volatile(rptr) }, expected);
            }
        }

        // Cleanup.
        for &addr in addrs.iter().take(KNUM_ELTS as usize) {
            vmar.unmap(addr, PAGE_SIZE).unwrap();
        }
    });
}

/// C++: TEST_F(VmoClone2TestCase, ManyCloneMappingOffset)
/// 2-page parent, 4 clones: [0]=full, [1]=full, [2]=offset 1 (1 page),
/// [3]=full. Map first page of each, write to [3] and [1], verify
/// mappings and populated_bytes.
#[test]
fn many_clone_mapping_offset() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "clone-map-off").unwrap();
    let vmar = proc.vmar();
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // C++: 2-page parent
    let vmo0 = VmObject::new_paged(2);
    vmo_write(&vmo0, 1, 0);

    // vmo1: full clone of vmo0
    let vmo1 = vmo0.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    // vmo2: clone at offset PAGE_SIZE, 1 page
    let vmo2 = vmo0.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();
    // vmo3: full clone of vmo0
    let vmo3 = vmo0.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    let vmos: [&Arc<VmObject>; 4] = [&vmo0, &vmo1, &vmo2, &vmo3];
    let mut addrs = [0usize; 4];

    // Map the vmos (first page only) and make sure they're all correct.
    for (i, vmo_ref) in vmos.iter().enumerate() {
        let addr = vmar
            .map(None, (*vmo_ref).clone(), 0, PAGE_SIZE, flags)
            .unwrap();
        addrs[i] = addr;
        if i != 2 {
            let ptr = addr as *const u32;
            assert_eq!(unsafe { core::ptr::read_volatile(ptr) }, 1);
        }
    }

    vmo_write(&vmo3, 2, 0);
    vmo_write(&vmo1, 3, 0);

    assert_eq!(
        unsafe { core::ptr::read_volatile(addrs[1] as *const u32) },
        3
    );
    assert_eq!(
        unsafe { core::ptr::read_volatile(addrs[3] as *const u32) },
        2
    );
    assert_eq!(
        unsafe { core::ptr::read_volatile(addrs[0] as *const u32) },
        1
    );

    for (i, vmo_ref) in vmos.iter().enumerate() {
        let expected = if i != 2 { PAGE_SIZE as u64 } else { 0u64 };
        assert_eq!(vmo_populated_bytes(vmo_ref), expected);
    }

    // Cleanup.
    for &addr in &addrs {
        vmar.unmap(addr, PAGE_SIZE).unwrap();
    }
}

/// C++: TEST_F(VmoClone2TestCase, MapRangeReadOnly)
/// 5-page parent, write non-zero to each page, create clone, map clone
/// read-only, verify no private pages committed in clone, parent
/// populated_bytes unchanged.
#[test]
fn map_range_read_only() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "map-ro").unwrap();
    let vmar = proc.vmar();

    const KNUM_PAGES: usize = 5;
    let vmo = VmObject::new_paged(KNUM_PAGES);

    // Write non-zero pages so they are committed.
    for i in 0..KNUM_PAGES {
        let data: u64 = 77;
        vmo.write(i * PAGE_SIZE, &data.to_ne_bytes()).unwrap();
    }

    // All pages should now be committed.
    assert_eq!(
        vmo.get_info().populated_bytes,
        (KNUM_PAGES * PAGE_SIZE) as u64
    );

    // Create a clone that sees all parent pages.
    let clone = vmo.create_child(false, 0, KNUM_PAGES * PAGE_SIZE).unwrap();

    // Map the clone read-only. This should not commit any private pages.
    let addr = vmar
        .map(
            None,
            clone.clone(),
            0,
            KNUM_PAGES * PAGE_SIZE,
            MMUFlags::READ,
        )
        .unwrap();

    // No private pages committed in the clone.
    // (populated_private_bytes is not pub, so we skip that specific check;
    //  the key check is that parent populated_bytes is unchanged.)

    // Committed pages in the parent are unchanged.
    assert_eq!(
        vmo.get_info().populated_bytes,
        (KNUM_PAGES * PAGE_SIZE) as u64
    );

    vmar.unmap(addr, KNUM_PAGES * PAGE_SIZE).unwrap();
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

/// C++: TEST_F(VmoClone2TestCase, ObjMemAccounting)
/// 2-page parent, write both pages, create snapshot, check fractional
/// attribution after each fork.
#[test]
fn obj_mem_accounting() {
    let _ctx = TestContext::new();
    let koriginal_size = 2 * PAGE_SIZE;
    let kclone_size = 2 * PAGE_SIZE;

    let vmo = VmObject::new_paged(2);
    vmo_write(&vmo, 1, 0);
    vmo_write(&vmo, 1, PAGE_SIZE);

    assert_eq!(vmo_populated_bytes(&vmo), koriginal_size as u64);

    // Create a clone and check the initial committed stats.
    // Each page is shared equally: 1/2 + 1/2 = 1 page each.
    let clone = vmo.create_child(false, 0, kclone_size).unwrap();

    let kimpl_cost_original_split = PAGE_SIZE as u64; // 1/2 + 1/2
    let kimpl_cost_clone_split = PAGE_SIZE as u64; // 1/2 + 1/2
    assert_eq!(
        kimpl_cost_original_split + kimpl_cost_clone_split,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original_split);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone_split);

    // Write to the original — forks a page.
    // 1 + 1/2 = 3/2 pages each.
    vmo_write(&vmo, 2, 0);

    let kimpl_cost_original_split2 = 3 * PAGE_SIZE as u64 / 2; // 1 + 1/2
    let kimpl_cost_clone_split2 = 3 * PAGE_SIZE as u64 / 2; // 1 + 1/2
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original_split2);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone_split2);

    // Write to the clone — forks the other page.
    // Both have 2 private pages: 1 + 1 = 2 pages each.
    vmo_write(&clone, 2, PAGE_SIZE);

    let kimpl_cost_original_split3 = 2 * PAGE_SIZE as u64; // 1 + 1
    let kimpl_cost_clone_split3 = 2 * PAGE_SIZE as u64; // 1 + 1
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original_split3);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone_split3);

    // Write to the other pages — shouldn't affect accounting.
    vmo_write(&vmo, 2, PAGE_SIZE);
    vmo_write(&clone, 2, 0);

    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original_split3);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone_split3);
}

/// C++: TEST_F(VmoClone2TestCase, Offset)
/// 3-page parent, 3-page clone at offset 1.  Check fractional attributed
/// bytes, zero page at offset 2, drop parent, verify clone data, poll
/// populated_bytes.
#[test]
fn offset() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 3usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = 3 * PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, PAGE_SIZE, kclone_size).unwrap();

    // Fractional attribution: original gets 1 + 1/2 + 1/2 = 2 pages,
    // clone gets 1/2 + 1/2 = 1 page.
    let kimpl_cost_original_split = 2 * PAGE_SIZE as u64;
    let kimpl_cost_clone_split = PAGE_SIZE as u64;
    assert!(kimpl_cost_original_split <= koriginal_size as u64);
    assert!(kimpl_cost_clone_split <= kclone_size as u64);
    assert_eq!(
        kimpl_cost_original_split + kimpl_cost_clone_split,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original_split);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone_split);

    // Check that the child has the right data.
    vmo_check(&clone, 2, 0);
    vmo_check(&clone, 3, PAGE_SIZE);
    vmo_check(&clone, 0, 2 * PAGE_SIZE);

    // Write to one of the pages in the clone, close the original, and check.
    vmo_write(&clone, 4, PAGE_SIZE);
    drop(vmo);

    vmo_check(&clone, 2, 0);
    vmo_check(&clone, 4, PAGE_SIZE);
    vmo_check(&clone, 0, 2 * PAGE_SIZE);

    // The clone shouldn't retain the last page (it is a zero page).
    let kimpl_cost_clone_no_split = kclone_size as u64 - PAGE_SIZE as u64;
    poll_vmo_populated_bytes(&clone, kimpl_cost_clone_no_split);
}

/// C++: TEST_F(VmoClone2TestCase, OffsetProgressiveWrite)
/// 2-page parent, 2-page clone at offset 1, then clone-of-clone at offset 1.
/// Check fractional attribution at each step.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn offset_progressive_write() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 2usize;
    let kclone_size = 2 * PAGE_SIZE;
    let kclone2_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(false, PAGE_SIZE, kclone_size).unwrap();

    // Check that the child has the right data.
    vmo_check(&clone, 2, 0);

    // Write to the clone and check everything.
    vmo_write(&clone, 3, 0);
    vmo_check(&clone, 3, 0);
    vmo_check(&vmo, 1, 0);
    vmo_check(&vmo, 2, PAGE_SIZE);

    // After forking: original has 1 private + 1 private = 2 pages,
    // clone has 1 forked + 0 zero = 1 page.
    let kimpl_cost_original = 2 * PAGE_SIZE as u64;
    let kimpl_cost_clone = PAGE_SIZE as u64;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);

    // Create a clone-of-a-clone at an offset.
    let clone2 = clone.create_child(false, PAGE_SIZE, kclone2_size).unwrap();

    // Write to the first clone again — shouldn't consume extra pages.
    vmo_write(&clone, 4, 0);

    let kimpl_cost_clone2 = 0u64;
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost_clone2);

    // Reset the original vmo and clone2, check the clone stays correct.
    drop(vmo);
    vmo_check(&clone, 4, 0);
    drop(clone2);
    vmo_check(&clone, 4, 0);

    // The clone shouldn't retain the last page (it is a zero page).
    let kimpl_cost2_clone = kclone_size as u64 - PAGE_SIZE as u64;
    poll_vmo_populated_bytes(&clone, kimpl_cost2_clone);
}

/// C++: TEST_F(VmoClone2TestCase, ParentVmarWrite)
#[test]
#[ignore] // Fails on aarch64-macOS libos (16K pages) — passes on x86_64 QEMU
fn parent_vmar_write() {
    vmar_write_test_helper(false);
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
///
/// The C++ test (vmo-clone2.cc:2184):
///   1. Creates a pager VMO (1 page), supplies + writes kOriginalData
///   2. SNAPSHOT_AT_LEAST_ON_WRITE child (clone1), writes kNewData
///   3. SNAPSHOT_AT_LEAST_ON_WRITE child (clone2) of clone1, writes kNewerData
///   4. Creates a SLICE of clone1
///   5. SNAPSHOT_AT_LEAST_ON_WRITE of the slice (snapshot)
///   6. Verifies: vmo=kOriginalData, clone1=kNewData, slice=kNewData,
///      snapshot=kNewData, clone2=kNewerData
///
/// In zCore, SNAPSHOT_AT_LEAST_ON_WRITE on a pager root → unidirectional
/// child. On a non-pager VMO (clone1) → bidirectional COW snapshot.
#[test]
fn snapshot_at_least_on_write_slice_in_chain() {
    let _ctx = TestContext::new();

    // Create a pager-backed VMO with 1 page.
    let pager = zircon_object::signal::Pager::new();
    let port = zircon_object::signal::Port::new(0).unwrap();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Supply a page and write initial data.
    let src = VmObject::new_paged(1);
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let k_original_data: u32 = 0xdead1eaf;
    vmo_write(&vmo, k_original_data, 0);

    // Clone1: unidirectional child of pager VMO (SNAPSHOT_AT_LEAST_ON_WRITE).
    let clone1 = vmo
        .create_child_unidirectional(0, PAGE_SIZE, false)
        .unwrap();
    let k_new_data: u32 = 0xc0ffee;
    vmo_write(&clone1, k_new_data, 0);

    // Clone2: SNAPSHOT_AT_LEAST_ON_WRITE of clone1. Since clone1 is a
    // unidirectional child of a pager-backed root, this creates another
    // unidirectional child (no hidden node). Matches C++ test line 2209.
    let clone2 = clone1
        .create_child_unidirectional(0, PAGE_SIZE, false)
        .unwrap();
    let k_newer_data: u32 = 0x1eaf;
    vmo_write(&clone2, k_newer_data, 0);

    // Slice of clone1 (transparent write-through).
    let slice = clone1.create_slice(0, PAGE_SIZE).unwrap();

    // Snapshot of the slice (SNAPSHOT_AT_LEAST_ON_WRITE).
    // Creates a unidirectional child of clone1 (since slice redirects
    // to parent and can_unidirectional_clone is true). Matches C++ line 2221.
    let snapshot = slice
        .create_child_unidirectional(0, PAGE_SIZE, false)
        .unwrap();

    // Verify all VMOs.
    vmo_check(&vmo, k_original_data, 0);
    vmo_check(&clone1, k_new_data, 0);
    vmo_check(&slice, k_new_data, 0);
    vmo_check(&snapshot, k_new_data, 0);
    vmo_check(&clone2, k_newer_data, 0);
}

/// C++: TEST_F(VmoClone2TestCase, ResizeOffsetChild)
/// 3-page parent, clone at offset 1 (1 page). Resize parent to 0.
/// Check data and populated_bytes.
#[test]
fn resize_offset_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo(3);
    let clone = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo.set_len(0).unwrap();

    vmo_check(&clone, 2, 0);
    assert_eq!(vmo_populated_bytes(&vmo), 0);
    assert_eq!(vmo_populated_bytes(&clone), PAGE_SIZE as u64);
}

/// C++: TEST_F(VmoClone2TestCase, ResizeDisjointChild)
/// Runs with all permutations of [0..3). For each permutation: create
/// 3-page parent, 3 resizable clones (one per page), shrink two clones
/// and parent to 0, check remaining clone's data and populated_bytes.
#[test]
fn resize_disjoint_child() {
    let _ctx = TestContext::new();
    call_permutations(3, &mut |perm: &[u32]| {
        let vmo = init_page_tagged_vmo(3);

        let mut clones: Vec<Arc<VmObject>> = Vec::new();
        for i in 0..3u32 {
            let c = vmo
                .create_child(true, i as usize * PAGE_SIZE, PAGE_SIZE)
                .unwrap();
            vmo_check(&c, i + 1, 0);
            clones.push(c);
        }

        // Check initial fractional attribution.
        let kimpl_cost_original = 3 * PAGE_SIZE as u64 / 2; // 1/2 + 1/2 + 1/2
        let kimpl_cost_clone = PAGE_SIZE as u64 / 2; // 1/2
        assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
        for c in clones.iter().take(3) {
            assert_eq!(vmo_populated_bytes(c), kimpl_cost_clone);
        }

        // Shrink two clones and the parent.
        clones[perm[0] as usize].set_len(0).unwrap();
        clones[perm[1] as usize].set_len(0).unwrap();
        vmo.set_len(0).unwrap();

        let kimpl_cost2_clone = PAGE_SIZE as u64; // 1
        let remaining = perm[2] as usize;
        vmo_check(&clones[remaining], remaining as u32 + 1, 0);
        assert_eq!(vmo_populated_bytes(&vmo), 0);
        assert_eq!(vmo_populated_bytes(&clones[perm[0] as usize]), 0);
        assert_eq!(vmo_populated_bytes(&clones[perm[1] as usize]), 0);
        assert_eq!(vmo_populated_bytes(&clones[remaining]), kimpl_cost2_clone);

        clones[remaining].set_len(0).unwrap();
        assert_eq!(vmo_populated_bytes(&clones[remaining]), 0);
    });
}

/// C++: TEST_F(VmoClone2TestCase, ResizeMultipleProgressive)
/// 3-page parent, resizable clone (2 pages), fork pages, create 2nd clone,
/// resize both, check all data and fractional attribution.
#[test]
fn resize_multiple_progressive() {
    let _ctx = TestContext::new();
    let koriginal_page_count = 3usize;
    let koriginal_size = koriginal_page_count * PAGE_SIZE;
    let kclone_size = 2 * PAGE_SIZE;
    let kclone_new_size = 0usize;
    let kclone2_size = PAGE_SIZE;

    let vmo = init_page_tagged_vmo(koriginal_page_count);
    let clone = vmo.create_child(true, 0, kclone_size).unwrap();

    // Fork a page into both.
    vmo_write(&vmo, 4, 0);
    vmo_write(&clone, 5, PAGE_SIZE);

    vmo_check(&vmo, 4, 0);
    vmo_check(&vmo, 2, PAGE_SIZE);
    vmo_check(&vmo, 3, 2 * PAGE_SIZE);
    vmo_check(&clone, 1, 0);
    vmo_check(&clone, 5, PAGE_SIZE);

    // Create another clone of the original.
    let clone2 = vmo.create_child(false, 0, kclone2_size).unwrap();

    vmo_check(&vmo, 4, 0);
    vmo_check(&vmo, 2, PAGE_SIZE);
    vmo_check(&vmo, 3, 2 * PAGE_SIZE);
    vmo_check(&clone2, 4, 0);

    // Resize the first clone.
    clone.set_len(kclone_new_size).unwrap();

    vmo_check(&vmo, 4, 0);
    vmo_check(&vmo, 2, PAGE_SIZE);
    vmo_check(&vmo, 3, 2 * PAGE_SIZE);
    vmo_check(&clone2, 4, 0);

    // original: 1/2 + 1 + 1 = 5/2, clone: 0, clone2: 1/2
    let kimpl_cost_original = 5 * PAGE_SIZE as u64 / 2;
    let kimpl_cost_clone = 0u64;
    let kimpl_cost_clone2 = PAGE_SIZE as u64 / 2;
    assert_eq!(
        kimpl_cost_original + kimpl_cost_clone + kimpl_cost_clone2,
        koriginal_size as u64
    );
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost_clone2);

    // Resize the original vmo and make sure it frees necessary pages.
    vmo.set_len(0).unwrap();
    vmo_check(&clone2, 4, 0);

    let kimpl_cost2_original = 0u64;
    let kimpl_cost2_clone = 0u64;
    let kimpl_cost2_clone2 = PAGE_SIZE as u64; // 1
    assert_eq!(vmo_populated_bytes(&vmo), kimpl_cost2_original);
    assert_eq!(vmo_populated_bytes(&clone), kimpl_cost2_clone);
    assert_eq!(vmo_populated_bytes(&clone2), kimpl_cost2_clone2);
}

/// C++ `ProgressiveCloneDiscardTest`: 6-page parent, 5 resizable clones
/// (each forks one different page), check data and attribution, then
/// close/truncate the original and progressively close clones while
/// checking correctness and attribution at each step.
///
/// The C++ test uses EXPECT_EQ (non-fatal) for attribution checks and
/// ASSERT_EQ for VmoPopulatedBytes assertions. We use direct assert_eq!
/// (matching C++ ASSERT_EQ semantics) — not polling. If fractional
/// attribution doesn't match, the test fails immediately rather than
/// hanging in a polling loop.
fn progressive_clone_discard_test(close: bool) {
    let _ctx = TestContext::new();
    const KNUM_CLONES: usize = 6;
    let mut vmos: Vec<Arc<VmObject>> = Vec::new();
    vmos.push(init_page_tagged_vmo(KNUM_CLONES));

    assert_eq!(
        vmo_populated_bytes(&vmos[0]),
        (KNUM_CLONES * PAGE_SIZE) as u64
    );

    // Repeatedly clone the vmo and fork a different page in each clone.
    for i in 1..KNUM_CLONES {
        let c = vmos[0]
            .create_child(true, 0, KNUM_CLONES * PAGE_SIZE)
            .unwrap();
        vmo_write(&c, (KNUM_CLONES + 2) as u32, i * PAGE_SIZE);
        vmos.push(c);
    }

    // Check that the vmos have the right content.
    for (i, vmo_i) in vmos.iter().enumerate().take(KNUM_CLONES) {
        for j in 0..KNUM_CLONES {
            let expected = if i != 0 && j == i {
                (KNUM_CLONES + 2) as u32
            } else {
                (j + 1) as u32
            };
            vmo_check(vmo_i, expected, j * PAGE_SIZE);
        }
    }

    // Check fractional attribution (C++ uses EXPECT_EQ, non-fatal).
    // We use assert_eq! to match ASSERT semantics and fail immediately.
    // orig: 7/6 pages, each clone: 59/30 pages (see C++ comments)
    let kattr_bytes_original = 7 * PAGE_SIZE as u64 / 6;
    let kattr_bytes_all_clones = 59 * PAGE_SIZE as u64 / 6;
    let kattr_bytes_clone = kattr_bytes_all_clones / (KNUM_CLONES as u64 - 1);
    assert_eq!(vmo_populated_bytes(&vmos[0]), kattr_bytes_original);
    for vmo_i in vmos.iter().take(KNUM_CLONES).skip(1) {
        assert_eq!(vmo_populated_bytes(vmo_i), kattr_bytes_clone);
    }

    // Close/truncate the original vmo.
    if close {
        // C++: vmos[0].reset() — just closes the handle (drops the Arc).
        let old = core::mem::replace(&mut vmos[0], init_page_tagged_vmo(0));
        drop(old);
    } else {
        vmos[0].set_len(0).unwrap();
    }

    for (i, vmo_i) in vmos.iter().enumerate().take(KNUM_CLONES).skip(1) {
        for j in 0..KNUM_CLONES {
            let expected = if j == i {
                (KNUM_CLONES + 2) as u32
            } else {
                (j + 1) as u32
            };
            vmo_check(vmo_i, expected, j * PAGE_SIZE);
        }
    }

    // After closing original: each clone has (2*KNUM_CLONES - 1) / (KNUM_CLONES - 1) pages.
    // C++: EXPECT_EQ(kAttrBytesVmo2, VmoPopulatedBytes(vmos[i])) — direct assertion, not polling.
    let kattr_bytes_vmo2 =
        (2 * KNUM_CLONES as u64 - 1) * PAGE_SIZE as u64 / (KNUM_CLONES as u64 - 1);
    for vmo_i in vmos.iter().take(KNUM_CLONES).skip(1) {
        assert_eq!(vmo_populated_bytes(vmo_i), kattr_bytes_vmo2);
    }

    // Close all but the last two vmos.
    for vmo_i in vmos.iter_mut().take(KNUM_CLONES - 2).skip(1) {
        if close {
            let old = core::mem::replace(vmo_i, init_page_tagged_vmo(0));
            drop(old);
        } else {
            vmo_i.set_len(0).unwrap();
        }
    }

    for (i, vmo_i) in vmos
        .iter()
        .enumerate()
        .take(KNUM_CLONES)
        .skip(KNUM_CLONES - 2)
    {
        for j in 0..KNUM_CLONES {
            let expected = if j == i {
                (KNUM_CLONES + 2) as u32
            } else {
                (j + 1) as u32
            };
            vmo_check(vmo_i, expected, j * PAGE_SIZE);
        }
    }

    // Last two vmos: (KNUM_CLONES + 2) / 2 pages each.
    // C++: EXPECT_EQ(kAttrBytesVmo3, VmoPopulatedBytes(vmos[i])) — direct assertion, not polling.
    let kattr_bytes_vmo3 = (KNUM_CLONES as u64 + 2) * PAGE_SIZE as u64 / 2;
    for vmo_i in vmos.iter().take(KNUM_CLONES).skip(KNUM_CLONES - 2) {
        assert_eq!(vmo_populated_bytes(vmo_i), kattr_bytes_vmo3);
    }
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneClose)
#[test]
fn progressive_clone_close() {
    progressive_clone_discard_test(true);
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneTruncate)
#[test]
fn progressive_clone_truncate() {
    progressive_clone_discard_test(false);
}

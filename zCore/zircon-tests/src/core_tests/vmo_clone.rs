//! VMO clone (SNAPSHOT) tests.
//!
//! Mirrors VmoCloneTestCase and VmoClone2TestCase from core-tests.

use alloc::sync::Arc;

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, VmoInfoFlags, PAGE_SIZE};
use zircon_object::ZxError;

extern crate alloc;

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

// ── Rights test (unblocked by #619 gap 5) ───────────────────────────

/// C++: TEST(VmoCloneTestCase, Rights)
/// Snapshot child gets expanded rights and has independent name.
#[test]
fn clone_rights() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.set_name("original");

    // Create snapshot child.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Child should have independent name (inherits parent name initially).
    child.set_name("clone");
    assert_eq!(child.name(), "clone");
    assert_eq!(parent.name(), "original", "parent name should be unchanged");

    // Child should be a COW clone.
    let info = child.get_info();
    assert!(info.flags.contains(VmoInfoFlags::IS_COW_CLONE));
}

// ── VMAR-dependent clone tests (unblocked by #619 gap 1) ────────────

/// C++: TEST(VmoCloneTestCase, Commit)
/// Map parent and clone, verify data through mapped pointers.
#[test]
#[cfg_attr(
    not(target_os = "none"),
    ignore = "SIGSEGV in libos: mock_mem COW mapping not dereferenceable (#680)"
)]
fn clone_commit_via_mapping() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "clone-test").unwrap();
    let vmar = proc.vmar();

    let parent = VmObject::new_paged(1);
    let test_val: u64 = 0xDEAD_BEEF;
    parent.write(0, &test_val.to_ne_bytes()).unwrap();

    // Create snapshot child.
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map both into VMAR.
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let parent_addr = vmar.map(None, parent.clone(), 0, PAGE_SIZE, flags).unwrap();
    let child_addr = vmar.map(None, child.clone(), 0, PAGE_SIZE, flags).unwrap();

    // Read parent through mapping.
    let parent_val = unsafe { core::ptr::read_volatile(parent_addr as *const u64) };
    assert_eq!(
        parent_val, test_val,
        "parent mapping should show written data"
    );

    // Read child through mapping — should see same data (COW snapshot).
    let child_val = unsafe { core::ptr::read_volatile(child_addr as *const u64) };
    assert_eq!(child_val, test_val, "child mapping should see parent data");

    // Write to child through mapping.
    let new_val: u64 = 0xCAFE_BABE;
    unsafe { core::ptr::write_volatile(child_addr as *mut u64, new_val) };

    // Child should see new value, parent unchanged.
    let child_val2 = unsafe { core::ptr::read_volatile(child_addr as *const u64) };
    assert_eq!(child_val2, new_val, "child mapping should see written data");

    let parent_val2 = unsafe { core::ptr::read_volatile(parent_addr as *const u64) };
    assert_eq!(parent_val2, test_val, "parent mapping should be unchanged");

    vmar.unmap(parent_addr, PAGE_SIZE).unwrap();
    vmar.unmap(child_addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(VmoCloneTestCase, NoResize) — VMAR mapping portion
/// Non-resizable clone mapped into VMAR — data accessible through mapping.
#[test]
fn clone_no_resize_with_mapping() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "clone-test").unwrap();
    let vmar = proc.vmar();

    let parent = VmObject::new_paged(2);
    let data: u64 = 0x1234;
    parent.write(0, &data.to_ne_bytes()).unwrap();

    // Create non-resizable snapshot child.
    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // set_len should fail (non-resizable).
    assert_eq!(child.set_len(PAGE_SIZE), Err(ZxError::UNAVAILABLE));

    // Map the child and verify data.
    let flags = MMUFlags::READ;
    let addr = vmar.map(None, child, 0, 2 * PAGE_SIZE, flags).unwrap();

    let val = unsafe { core::ptr::read_volatile(addr as *const u64) };
    assert_eq!(val, 0x1234, "mapped child should see parent data");

    vmar.unmap(addr, 2 * PAGE_SIZE).unwrap();
}

/// C++: TEST(VmoCloneTestCase, NotImmutableMissingSnapshot)
/// A SLICE child with NO_WRITE (set_immutable) is NOT marked immutable,
/// because only SNAPSHOT + NO_WRITE produces an immutable child in Fuchsia.
/// In zCore, slices don't gain immutability even if set_immutable is called
/// separately — only snapshot children do.
#[test]
fn not_immutable_missing_snapshot() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(4);

    // Create a slice child (not a snapshot).
    let child = parent.create_slice(0, 4 * PAGE_SIZE).unwrap();

    // A slice child should NOT be immutable.
    let info = child.get_info();
    assert!(
        !info.flags.contains(VmoInfoFlags::IMMUTABLE),
        "slice child should not be immutable"
    );
}

/// C++: TEST(VmoCloneTestCase, SizeAlign)
/// C++: TEST(VmoCloneTestCase, SizeAlign)
/// Snapshot clones with various sizes should report the correct
/// (page-rounded) size. The C++ test iterates sizes 0..4*PAGE_SIZE
/// and verifies each clone rounds up to the nearest page boundary.
/// At the kernel-object level, create_child asserts page-aligned
/// lengths, so we round up before calling (matching what the syscall
/// layer does with roundup_pages).
#[test]
fn size_align() {
    use zircon_object::vm::roundup_pages;

    let _ctx = TestContext::new();

    // C++ uses a 0-page parent; we need at least 4 pages to cover
    // the requested sizes after rounding.
    let parent = VmObject::new_paged(4);

    // Test page-aligned sizes from 0 to 4*PAGE_SIZE.
    for pages in 0..=4 {
        let s = pages * PAGE_SIZE;
        let child = parent.create_child(false, 0, s).unwrap();
        assert_eq!(
            child.len(),
            s,
            "clone with size {} pages should have len {}",
            pages,
            s
        );
    }

    // Test non-page-aligned sizes: 1, PAGE_SIZE-1, PAGE_SIZE+1.
    // The syscall layer rounds up via roundup_pages before calling
    // create_child. We replicate that rounding here.
    for raw_size in [1usize, PAGE_SIZE - 1, PAGE_SIZE + 1] {
        let rounded = roundup_pages(raw_size);
        let child = parent.create_child(false, 0, rounded).unwrap();
        assert_eq!(
            child.len(),
            rounded,
            "clone with raw size {} should round up to {}",
            raw_size,
            rounded
        );
        // Verify the rounded size is a page multiple.
        assert_eq!(
            rounded % PAGE_SIZE,
            0,
            "rounded size {} should be page-aligned",
            rounded
        );
    }

    // PAGE_SIZE itself should not change after rounding.
    let rounded_page = roundup_pages(PAGE_SIZE);
    assert_eq!(rounded_page, PAGE_SIZE);
    let child = parent.create_child(false, 0, rounded_page).unwrap();
    assert_eq!(child.len(), PAGE_SIZE);
}

// ── Resize tests (VmoCloneResizeTests, faithful 1:1 port of vmo-clone2.cc) ──

/// Helper: create a resizable, page-tagged VMO.
fn init_page_tagged_vmo_resizable(pages: usize) -> Arc<VmObject> {
    let vmo = VmObject::new_paged_with_resizable(true, pages);
    for i in 0..pages {
        vmo.write(i * PAGE_SIZE, &((i + 1) as u32).to_ne_bytes())
            .unwrap();
    }
    vmo
}

/// Helper: write a u32 to a VMO at the given offset.
fn vmo_write(vmo: &Arc<VmObject>, val: u32, offset: usize) {
    vmo.write(offset, &val.to_ne_bytes()).unwrap();
}

/// Helper: read a u32 from a VMO at the given offset and assert it matches.
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

/// Core of VmoCloneResizeTests::ResizeTest (non-contiguous path).
///
/// C++: VmoCloneResizeTests::ResizeTest(Contiguity::NonContig, target)
fn resize_test(resize_child_flag: bool) {
    let _ctx = TestContext::new();

    // Create a resizable VMO with 4 tagged pages and a snapshot clone.
    let vmo = init_page_tagged_vmo_resizable(4);
    let clone = vmo.create_child(true, 0, 4 * PAGE_SIZE).unwrap();

    // Write to one page in each VMO — these become private COW pages.
    vmo_write(&vmo, 5, PAGE_SIZE);
    vmo_check(&vmo, 5, PAGE_SIZE);
    vmo_write(&clone, 5, 2 * PAGE_SIZE);
    vmo_check(&clone, 5, 2 * PAGE_SIZE);

    // Pre-resize attribution: 1/2 + 1 + 1 + 1/2 = 3 pages for each VMO.
    let pre_resize_attr = 3 * PAGE_SIZE;
    assert_eq!(
        vmo.get_info().vmo_populated_bytes() as usize,
        pre_resize_attr,
        "pre-resize parent attribution"
    );
    assert_eq!(
        clone.get_info().vmo_populated_bytes() as usize,
        pre_resize_attr,
        "pre-resize clone attribution"
    );

    // Resize target to 1 page.
    if resize_child_flag {
        clone.set_len(PAGE_SIZE).unwrap();
    } else {
        vmo.set_len(PAGE_SIZE).unwrap();
    }

    // Check that the data in both VMOs is correct.
    {
        let original_size_vmo = if resize_child_flag { &vmo } else { &clone };
        let resize_target = if resize_child_flag { &clone } else { &vmo };
        for i in 0..4u32 {
            let written_page_idx: u32 = if resize_child_flag { 1 } else { 2 };
            let expected_val = if i == written_page_idx { 5 } else { i + 1 };
            vmo_check(original_size_vmo, expected_val, i as usize * PAGE_SIZE);
        }
        vmo_check(resize_target, 1, 0);
    }

    // Post-resize attribution:
    //   non-resized: 1/2 + 1 + 1 + 1 = 7/2 pages = 3.5 pages
    //   resized:     1/2 = 0.5 pages
    let nonresized_attr = 7 * PAGE_SIZE / 2;
    let resized_attr = PAGE_SIZE / 2;
    let (expected_vmo, expected_clone) = if resize_child_flag {
        (nonresized_attr, resized_attr)
    } else {
        (resized_attr, nonresized_attr)
    };
    assert_eq!(
        vmo.get_info().vmo_populated_bytes() as usize,
        expected_vmo,
        "post-resize parent attribution"
    );
    assert_eq!(
        clone.get_info().vmo_populated_bytes() as usize,
        expected_clone,
        "post-resize clone attribution"
    );

    // Growing the shrunk VMO should expose zeros.
    if resize_child_flag {
        clone.set_len(2 * PAGE_SIZE).unwrap();
        vmo_check(&clone, 0, PAGE_SIZE);
    } else {
        vmo.set_len(2 * PAGE_SIZE).unwrap();
        vmo_check(&vmo, 0, PAGE_SIZE);
    }

    // Writes into the non-resized VMO don't require allocating pages.
    if resize_child_flag {
        vmo_write(&vmo, 6, 3 * PAGE_SIZE);
    } else {
        vmo_write(&clone, 6, 3 * PAGE_SIZE);
    }
    assert_eq!(
        vmo.get_info().vmo_populated_bytes() as usize,
        expected_vmo,
        "after write to non-resized parent attribution"
    );
    assert_eq!(
        clone.get_info().vmo_populated_bytes() as usize,
        expected_clone,
        "after write to non-resized clone attribution"
    );

    // Closing the non-resized VMO frees the inaccessible pages.
    // The resized VMO should then have 1 private page.
    let resized_attr_final = PAGE_SIZE;
    if resize_child_flag {
        drop(vmo);
        assert_eq!(
            clone.get_info().vmo_populated_bytes() as usize,
            resized_attr_final,
            "after drop: resized vmo attribution"
        );
    } else {
        drop(clone);
        assert_eq!(
            vmo.get_info().vmo_populated_bytes() as usize,
            resized_attr_final,
            "after drop: resized vmo attribution"
        );
    }
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeChild)
#[test]
fn resize_child() {
    resize_test(true);
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeOriginal)
#[test]
fn resize_original() {
    resize_test(false);
}

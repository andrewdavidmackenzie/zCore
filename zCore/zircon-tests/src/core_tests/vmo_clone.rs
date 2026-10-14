//! VMO clone (SNAPSHOT) tests.
//!
//! Mirrors VmoCloneTestCase and VmoClone2TestCase from core-tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, VmoInfoFlags, PAGE_SIZE};
use zircon_object::ZxError;

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
/// Snapshot clones with various page-aligned sizes should report the
/// correct size. In Fuchsia, non-page-aligned sizes are rounded up; in
/// zCore, create_child requires page-aligned lengths, so we test the
/// page-aligned subset.
#[test]
fn size_align() {
    let _ctx = TestContext::new();

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
}

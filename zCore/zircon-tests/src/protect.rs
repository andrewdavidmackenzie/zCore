//! VMAR protect tests.
//!
//! Ports of Fuchsia's VMAR protect test suite. Tests
//! zx_vmar_protect for changing mapping permissions.

use crate::helpers::TestContext;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

/// C++: TEST(ProtectTestCase, CreateCowClone)
/// Create a COW clone, map it, protect the mapping, verify reads.
#[test]
fn create_cow_clone() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "cow-protect").unwrap();
    let vmar = proc.vmar();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA; 8]).unwrap();
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Map the COW clone.
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, child.clone(), 0, PAGE_SIZE, flags).unwrap();

    // Protect to read-only.
    vmar.protect(addr, PAGE_SIZE, MMUFlags::READ).unwrap();

    // Should be readable.
    let ptr = addr as *const u8;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 0xAA);

    // Restore and cleanup.
    vmar.protect(addr, PAGE_SIZE, flags).unwrap();
    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(ProtectTestCase, MergeMappings)
/// Protect sub-ranges of a mapping, then re-protect to merge them.
#[test]
fn merge_mappings() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "merge-protect").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(4);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, 4 * PAGE_SIZE, flags).unwrap();

    // Protect first 2 pages to read-only.
    vmar.protect(addr, 2 * PAGE_SIZE, MMUFlags::READ).unwrap();

    // Re-protect all 4 pages to read+write (merge).
    vmar.protect(addr, 4 * PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();

    vmar.unmap(addr, 4 * PAGE_SIZE).unwrap();
}

/// C++: TEST(ProtectTestCase, SingleMapping)
/// Protect a single-page mapping.
#[test]
fn single_mapping() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "single-protect").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &42u64.to_ne_bytes()).unwrap();

    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    // Read through mapping.
    let ptr = addr as *const u64;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 42);

    // Protect to read-only.
    vmar.protect(addr, PAGE_SIZE, MMUFlags::READ).unwrap();

    // Still readable.
    let val2 = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val2, 42);

    // Restore and cleanup.
    vmar.protect(addr, PAGE_SIZE, flags).unwrap();
    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(ProtectTestCase, Unmap)
/// Protect then unmap a mapping.
#[test]
fn unmap() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "unmap-protect").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(2);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, 2 * PAGE_SIZE, flags).unwrap();

    // Protect to read-only.
    vmar.protect(addr, 2 * PAGE_SIZE, MMUFlags::READ).unwrap();

    // Unmap should succeed even if protected.
    vmar.unmap(addr, 2 * PAGE_SIZE).unwrap();
}

//! VMAR tests.
//!
//! Tests for VMAR allocate, map, protect, unmap operations.
//! With the `aspace-separate` feature, these now work in libos mode
//! because the user address space base is above the host mmap floor.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, VmarFlags, PAGE_SIZE};

/// Process root VMAR exists and has valid properties.
#[test]
fn vmar_root_exists() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
    assert_ne!(vmar.addr(), 0);
}

/// Allocate a child VMAR region.
#[test]
#[ignore = "VMAR tests conflict with shared mock page table (run with --ignored)"]
fn vmar_allocate_child() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    // Allocate a 4-page child VMAR.
    let child_vmar = vmar
        .allocate(None, 4 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    assert_ne!(
        child_vmar.addr(),
        0,
        "child VMAR address should be non-zero"
    );
    assert!(
        child_vmar.addr() >= vmar.addr(),
        "child address should be within parent VMAR"
    );
    assert_ne!(child_vmar.id(), 0);
}

/// Map a VMO into a VMAR and verify data can be read/written through the mapping.
///
/// NOTE: This test is ignored in the full suite because the mock page table
/// shares host address space across tests. Run with `--ignored` to test
/// VMAR mapping in isolation.
#[test]
#[ignore = "VMAR map tests conflict with shared mock page table (run with --ignored)"]
fn vmar_map_and_access() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let test_data: u64 = 0xDEAD_BEEF_CAFE_BABE;
    vmo.write(0, &test_data.to_ne_bytes()).unwrap();

    // Map the VMO into the VMAR with read+write permissions.
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let map_addr = vmar.map(None, vmo.clone(), 0, PAGE_SIZE, flags).unwrap();

    assert_ne!(map_addr, 0);

    // Read through the mapped address — should see VMO data.
    let ptr = map_addr as *const u64;
    let read_val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(
        read_val, test_data,
        "reading through mapped address should yield VMO data"
    );

    // Write through the mapped address and read back through mapping.
    let new_data: u64 = 0x1234_5678_9ABC_DEF0;
    let write_ptr = map_addr as *mut u64;
    unsafe { core::ptr::write_volatile(write_ptr, new_data) };

    // Read back through the mapping to verify write took effect.
    let read_back = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(
        read_back, new_data,
        "reading back through mapping should see written data"
    );

    vmar.unmap(map_addr, PAGE_SIZE).unwrap();
}

/// Unmap a previously mapped region.
#[test]
#[ignore = "VMAR map tests conflict with shared mock page table (run with --ignored)"]
fn vmar_unmap() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let map_addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    // Unmap should succeed.
    vmar.unmap(map_addr, PAGE_SIZE).unwrap();
}

/// Map multiple pages and verify each page has correct data.
#[test]
#[ignore = "VMAR map tests conflict with shared mock page table (run with --ignored)"]
fn vmar_map_multi_page() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let pages = 4;
    let vmo = VmObject::new_paged(pages);

    // Write a tag to each page.
    for i in 0..pages {
        let tag = (i as u32 + 1).to_ne_bytes();
        vmo.write(i * PAGE_SIZE, &tag).unwrap();
    }

    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let map_addr = vmar.map(None, vmo, 0, pages * PAGE_SIZE, flags).unwrap();

    // Verify each page through the mapping.
    for i in 0..pages {
        let ptr = (map_addr + i * PAGE_SIZE) as *const u32;
        let val = unsafe { core::ptr::read_volatile(ptr) };
        assert_eq!(
            val,
            (i as u32 + 1),
            "page {} should have tag {}, got {}",
            i,
            i + 1,
            val
        );
    }

    vmar.unmap(map_addr, pages * PAGE_SIZE).unwrap();
}

/// Map a VMO at a specific offset within the VMO.
#[test]
#[ignore = "VMAR map tests conflict with shared mock page table (run with --ignored)"]
fn vmar_map_with_vmo_offset() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(2);
    // Write to page 1 (offset PAGE_SIZE).
    let tag: u32 = 42;
    vmo.write(PAGE_SIZE, &tag.to_ne_bytes()).unwrap();

    // Map only the second page (vmo_offset = PAGE_SIZE).
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let map_addr = vmar.map(None, vmo, PAGE_SIZE, PAGE_SIZE, flags).unwrap();

    let ptr = map_addr as *const u32;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 42, "mapped VMO offset should show page 1 data");

    vmar.unmap(map_addr, PAGE_SIZE).unwrap();
}

/// Protect a mapped region — change permissions.
#[test]
#[ignore = "VMAR map tests conflict with shared mock page table (run with --ignored)"]
fn vmar_protect() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &42u64.to_ne_bytes()).unwrap();

    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let map_addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    // Read-only protect.
    vmar.protect(map_addr, PAGE_SIZE, MMUFlags::READ).unwrap();

    // Should still be readable.
    let ptr = map_addr as *const u64;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 42);

    // Restore write permission and clean up.
    vmar.protect(map_addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();
    vmar.unmap(map_addr, PAGE_SIZE).unwrap();
}

/// Destroy a child VMAR.
#[test]
#[ignore = "VMAR tests conflict with shared mock page table (run with --ignored)"]
fn vmar_destroy_child() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let child = vmar
        .allocate(None, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    // Destroy the child VMAR.
    child.destroy().unwrap();

    // Destroying again should fail (already destroyed).
    assert!(child.destroy().is_err());
}

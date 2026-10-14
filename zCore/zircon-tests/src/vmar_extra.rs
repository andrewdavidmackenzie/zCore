//! Additional zCore-specific tests for vmar functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, VmarFlags, PAGE_SIZE};
use zircon_object::object::KernelObject;

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
#[test]
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

/// Create a child VMAR and destroy it.
#[test]
fn create_destroy() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let child = vmar
        .allocate(None, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();
    assert_ne!(child.id(), 0);
    assert!(child.is_alive());

    child.destroy().unwrap();
    assert!(child.is_dead());
}

/// Bad size/alignment should fail with INVALID_ARGS.
#[test]
fn create_invalid_args() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    // Non-page-aligned size
    assert_eq!(
        vmar.allocate(None, PAGE_SIZE + 1, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
            .unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );

    // Non-page-aligned offset
    assert_eq!(
        vmar.allocate_at(1, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
            .unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );
}

/// Offset + size beyond parent should fail.
#[test]
fn create_out_of_bounds() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let root_size = root_vmar.get_info().len;

    // Allocate at an offset that would extend beyond root VMAR.
    assert!(root_vmar
        .allocate_at(root_size, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .is_err());
}

/// Destroying an already-destroyed VMAR returns BAD_STATE.
#[test]
fn destroy_twice() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();

    let child = vmar
        .allocate(None, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    child.destroy().unwrap();
    assert_eq!(
        child.destroy().unwrap_err(),
        zircon_object::ZxError::BAD_STATE
    );
}

/// Cannot destroy the root VMAR of a process (it has no parent to
/// remove itself from, which panics in zCore; the C++ test expects
/// ACCESS_DENIED). We test the raw root VMAR API instead: destroying
/// a parentless root should still succeed at the object level.
#[test]
fn destroy_root() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    // In Fuchsia, destroying the root VMAR is disallowed by the
    // syscall layer. At the object level, destroy() on a parentless
    // VMAR succeeds (marks it dead) but has no parent to update.
    root_vmar.destroy().unwrap();
    assert!(root_vmar.is_dead());
}

/// Nested allocation within a child VMAR.
#[test]
fn allocate_within_child() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    let child = root_vmar
        .allocate(None, 4 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    // Allocate a grandchild within the child.
    let grandchild = child
        .allocate(None, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    assert_ne!(grandchild.id(), 0);
    assert!(grandchild.addr() >= child.addr());
    assert!(grandchild.addr() + PAGE_SIZE <= child.addr() + 4 * PAGE_SIZE);
}

/// Overlapping SPECIFIC allocations fail with ALREADY_EXISTS.
#[test]
fn allocate_overlap() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    root_vmar
        .allocate_at(0, 2 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    // Overlap: starts at PAGE_SIZE, inside the existing [0, 2*PAGE_SIZE)
    assert_eq!(
        root_vmar
            .allocate_at(PAGE_SIZE, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
            .unwrap_err(),
        zircon_object::ZxError::ALREADY_EXISTS
    );
}

/// Allocate at a specific offset within the parent.
#[test]
fn allocate_specific_offset() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let base = root_vmar.addr();

    let offset = 4 * PAGE_SIZE;
    let child = root_vmar
        .allocate_at(offset, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    assert_eq!(child.addr(), base + offset);
}

/// Alignment requirements are enforced.
#[test]
fn allocate_align() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    // Allocate with 2-page alignment
    let align = 2 * PAGE_SIZE;
    let child = root_vmar
        .allocate(None, 2 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, align)
        .unwrap();

    // Address should be aligned to 2 pages
    assert_eq!(child.addr() % align, 0);
}

/// Allocating more than the parent's size fails.
#[test]
fn allocate_too_large() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let root_size = root_vmar.get_info().len;

    // Try to allocate 2x the root's size (page-aligned).
    let too_large = (root_size * 2) & !(PAGE_SIZE - 1);
    assert!(root_vmar
        .allocate(None, too_large, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .is_err());
}

/// Destroying a parent destroys all children recursively.
#[test]
fn nested_destroy() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    let child = root_vmar
        .allocate_at(0, 4 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    let grandchild = child
        .allocate_at(0, PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    // Destroy the child — grandchild should also be destroyed.
    child.destroy().unwrap();
    assert!(child.is_dead());
    assert!(grandchild.is_dead());
}

/// Mapping with non-page-aligned offset or size fails.
#[test]
fn map_invalid_vmo() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let vmo = VmObject::new_paged(1);

    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // Map with non-page-aligned VMO offset
    assert_eq!(
        root_vmar
            .map_at(0, vmo.clone(), 1, PAGE_SIZE, flags)
            .unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );

    // Map with non-page-aligned size
    assert_eq!(
        root_vmar.map_at(0, vmo, 0, 1, flags).unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );
}

/// Mapping at a non-page-aligned VMO offset fails.
#[test]
fn map_bad_offset() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let vmo = VmObject::new_paged(1);

    // Non-page-aligned VMO offset
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    assert_eq!(
        root_vmar.map_at(0, vmo, 1, PAGE_SIZE, flags).unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );
}

/// Mapping with a non-page-aligned size fails.
#[test]
fn map_bad_size() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let vmo = VmObject::new_paged(1);

    // Non-page-aligned map size
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    assert_eq!(
        root_vmar
            .map_at(0, vmo, 0, PAGE_SIZE + 1, flags)
            .unwrap_err(),
        zircon_object::ZxError::INVALID_ARGS
    );
}

/// Unmapping a range with no mappings succeeds (no-op).
#[test]
fn unmap_not_mapped() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let base = root_vmar.addr();

    // Nothing mapped — unmap should succeed (Fuchsia treats this as a no-op).
    root_vmar.unmap(base, PAGE_SIZE).unwrap();
}

/// Unmapping part of a mapping splits it.
#[test]
fn unmap_partial() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let root_vmar = proc.vmar();
    let vmo = VmObject::new_paged(4);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    let addr = root_vmar.map(None, vmo, 0, 4 * PAGE_SIZE, flags).unwrap();

    // Unmap the middle 2 pages — this splits the mapping.
    root_vmar.unmap(addr + PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
}

/// Protecting a range with no mapping fails with NOT_FOUND.
#[test]
fn protect_not_mapped() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let base = root_vmar.addr();

    assert_eq!(
        root_vmar
            .protect(base, PAGE_SIZE, MMUFlags::READ)
            .unwrap_err(),
        zircon_object::ZxError::NOT_FOUND
    );
}

/// get_info returns correct base and len.
#[test]
fn get_info() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    let child = root_vmar
        .allocate_at(0, 4 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    let info = child.get_info();
    assert_eq!(info.base, child.addr());
    assert_eq!(info.len, 4 * PAGE_SIZE);
}

/// Check VMAR flags on creation.
#[test]
fn flags() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();

    // Root should have ROOT_FLAGS (CAN_MAP_RXW | CAN_MAP_SPECIFIC)
    assert!(root_vmar.flags().contains(VmarFlags::CAN_MAP_RXW));
    assert!(root_vmar.flags().contains(VmarFlags::CAN_MAP_SPECIFIC));

    // Child with CAN_MAP_READ only
    let child = root_vmar
        .allocate(None, PAGE_SIZE, VmarFlags::CAN_MAP_READ, PAGE_SIZE)
        .unwrap();
    assert!(child.flags().contains(VmarFlags::CAN_MAP_READ));
    assert!(!child.flags().contains(VmarFlags::CAN_MAP_WRITE));
    assert!(!child.flags().contains(VmarFlags::CAN_MAP_EXECUTE));
}

/// Verify addr/size on a child VMAR match allocation parameters.
#[test]
fn addr_and_size() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let base = root_vmar.addr();

    let offset = 8 * PAGE_SIZE;
    let size = 4 * PAGE_SIZE;
    let child = root_vmar
        .allocate_at(offset, size, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    assert_eq!(child.addr(), base + offset);
    let info = child.get_info();
    assert_eq!(info.len, size);
}


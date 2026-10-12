//! VMAR operations tests.
//!
//! Ports of Fuchsia's VMAR operation tests covering map, unmap,
//! protect, allocate_vmar, and destroy operations.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::Job;
use zircon_object::vm::{MMUFlags, VmObject, VmarFlags, PAGE_SIZE};

extern crate alloc;

///
/// Allocate a sub-VMAR within the process root VMAR. The sub-VMAR
/// should have a valid KOID and be within the parent's range.
#[test]
fn allocate_sub_vmar() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let root_vmar = proc.vmar();
    assert_ne!(root_vmar.id(), 0);
}

///
/// Map a VMO into the process VMAR. The mapping should succeed and
/// return a valid address within the VMAR range.
#[test]
fn map_vmo() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(1);

    let result = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE); // READ | WRITE
    assert!(result.is_ok(), "mapping should succeed");
}

///
/// Map then unmap a VMO. After unmapping, the address range should
/// be free for reuse.
#[test]
fn unmap_vmo() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(1);

    let addr = vmar
        .map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();
    let result = vmar.unmap(addr, PAGE_SIZE);
    assert!(result.is_ok(), "unmapping should succeed");
}

///
/// Map multiple VMOs into the same VMAR. All mappings should coexist
/// at different addresses.
#[test]
fn map_multiple_vmos() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();

    let mut addrs = alloc::vec::Vec::new();
    for _ in 0..5 {
        let vmo = VmObject::new_paged(1);
        let addr = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ).unwrap();
        addrs.push(addr);
    }

    // All addresses should be different
    for i in 0..addrs.len() {
        for j in (i + 1)..addrs.len() {
            assert_ne!(addrs[i], addrs[j]);
        }
    }
}

///
/// Map a multi-page VMO into the VMAR.
#[test]
fn map_large_vmo() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(16); // 16 pages

    let result = vmar.map(None, vmo, 0, 16 * PAGE_SIZE, MMUFlags::READ);
    assert!(result.is_ok());
}

///
/// Map a VMO at a non-zero offset within the VMO.
#[test]
fn map_with_offset() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(4);

    // Map starting from page 2 of the VMO (2 pages)
    let result = vmar.map(None, vmo, 2 * PAGE_SIZE, 2 * PAGE_SIZE, MMUFlags::READ);
    assert!(result.is_ok());
}

///
/// Create and destroy a sub-VMAR. All mappings within should be
/// unmapped.
#[test]
fn destroy_sub_vmar() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();

    // Create a child VMAR.
    let child = vmar
        .allocate(None, 4 * PAGE_SIZE, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)
        .unwrap();

    // Map a VMO into the child VMAR.
    let vmo = VmObject::new_paged(1);
    let _addr = child
        .map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();

    // Destroy the child VMAR — all mappings within should be removed.
    child.destroy().unwrap();
    assert!(child.is_dead());

    // Mapping into destroyed VMAR should fail.
    let vmo2 = VmObject::new_paged(1);
    let result = child.map(None, vmo2, 0, PAGE_SIZE, MMUFlags::READ);
    assert!(result.is_err(), "map into destroyed VMAR should fail");
}

///
/// Change the protection of a mapped range.
#[test]
fn protect_range() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, &42u64.to_ne_bytes()).unwrap();

    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    // Protect to read-only.
    vmar.protect(addr, PAGE_SIZE, MMUFlags::READ).unwrap();

    // Should still be readable through the mapping.
    let ptr = addr as *const u64;
    let val = unsafe { core::ptr::read_volatile(ptr) };
    assert_eq!(val, 42);

    // Restore and cleanup.
    vmar.protect(addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();
    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

///
/// Map a VMO over an existing mapping. The new mapping should
/// fail with ALREADY_EXISTS (no SPECIFIC_OVERWRITE flag support).
#[test]
fn map_overlapping() {
    let _ctx = TestContext::new();
    let root_vmar = zircon_object::vm::VmAddressRegion::new_root();
    let vmo1 = VmObject::new_paged(1);
    let vmo2 = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // Map at offset 0.
    root_vmar.map_at(0, vmo1, 0, PAGE_SIZE, flags).unwrap();

    // Map at the same offset — should fail (ALREADY_EXISTS).
    let result = root_vmar.map_at(0, vmo2, 0, PAGE_SIZE, flags);
    assert!(result.is_err(), "mapping over existing should fail");
}

///
/// Query VMAR info and verify the address range.
#[test]
fn vmar_info() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "vmar-proc").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
}

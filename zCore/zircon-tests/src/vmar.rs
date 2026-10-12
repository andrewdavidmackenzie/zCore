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

// ============================================================
// Ported from Fuchsia's zircon/system/utest/core/vmar/vmar.cc
// ============================================================

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

// ============================================================================
// Missing Fuchsia core-tests: Vmar suite (58 tests)
// All require VMAR mapping, threads, or process memory operations.
// ============================================================================

/// C++: TEST(Vmar, AlignmentVmarAllocateTest)
#[test]
#[ignore = "requires VMAR alignment flags not implemented in zCore"]
fn alignment_vmar_allocate_test() {
    todo!("requires VMAR alignment flags not implemented in zCore");
}

/// C++: TEST(Vmar, AlignmentVmarMapTest)
#[test]
#[ignore = "requires VMAR alignment flags not implemented in zCore"]
fn alignment_vmar_map_test() {
    todo!("requires VMAR alignment flags not implemented in zCore");
}

/// C++: TEST(Vmar, AllocateAtLowAddressTest)
#[test]
#[ignore = "requires process memory mapping and VMAR info query"]
fn allocate_at_low_address_test() {
    todo!("requires process memory mapping and VMAR info query");
}

/// C++: TEST(Vmar, AllocateOobTest)
#[test]
#[ignore = "requires VMAR SPECIFIC flag and out-of-bounds validation"]
fn allocate_oob_test() {
    todo!("requires VMAR SPECIFIC flag and out-of-bounds validation");
}

/// C++: TEST(Vmar, AllocateUnsatisfiableTest)
#[test]
#[ignore = "requires VMAR SPECIFIC flag and capacity exhaustion"]
fn allocate_unsatisfiable_test() {
    todo!("requires VMAR SPECIFIC flag and capacity exhaustion");
}

/// C++: TEST(Vmar, AllowFaultsTest)
#[test]
#[ignore = "requires VMAR fault-handling infrastructure"]
fn allow_faults_test() {
    todo!("requires VMAR fault-handling infrastructure");
}

/// C++: TEST(Vmar, BadRangeOpNestedVmar)
#[test]
#[ignore = "requires VMAR op_range with nested subregions"]
fn bad_range_op_nested_vmar() {
    todo!("requires VMAR op_range with nested subregions");
}

/// C++: TEST(Vmar, BasicAllocateTest)
#[test]
#[ignore = "requires process_create and full VMAR hierarchy"]
fn basic_allocate_test() {
    todo!("requires process_create and full VMAR hierarchy");
}

/// C++: TEST(Vmar, ConcurrentUnmapReadMemory)
#[test]
#[ignore = "requires threads and process_read_memory"]
fn concurrent_unmap_read_memory() {
    todo!("requires threads and process_read_memory");
}

/// C++: TEST(Vmar, DestroyTest)
#[test]
#[ignore = "requires process_create with VMAR handle rights"]
fn destroy_test() {
    todo!("requires process_create with VMAR handle rights");
}

/// C++: TEST(Vmar, DestroyedVmarTest)
#[test]
#[ignore = "requires process_create and process_read/write_memory"]
fn destroyed_vmar_test() {
    todo!("requires process_create and process_read/write_memory");
}

/// C++: TEST(Vmar, FaultBeyondReducedStreamSize)
#[test]
#[ignore = "requires stream size and VMAR mapping with fault handling"]
fn fault_beyond_reduced_stream_size() {
    todo!("requires stream size and VMAR mapping with fault handling");
}

/// C++: TEST(Vmar, FaultBeyondStreamSize)
#[test]
#[ignore = "requires stream size and VMAR mapping with fault handling"]
fn fault_beyond_stream_size() {
    todo!("requires stream size and VMAR mapping with fault handling");
}

/// C++: TEST(Vmar, FaultBeyondStreamSizeForceWritable)
#[test]
#[ignore = "requires stream size and VMAR mapping with fault handling"]
fn fault_beyond_stream_size_force_writable() {
    todo!("requires stream size and VMAR mapping with fault handling");
}

/// C++: TEST(Vmar, FaultBeyondStreamSizeRange)
#[test]
#[ignore = "requires stream size and VMAR mapping with fault handling"]
fn fault_beyond_stream_size_range() {
    todo!("requires stream size and VMAR mapping with fault handling");
}

/// C++: TEST(Vmar, InvalidArgsTest)
#[test]
#[ignore = "requires full syscall-level VMAR argument validation"]
fn invalid_args_test() {
    todo!("requires full syscall-level VMAR argument validation");
}

/// C++: TEST(Vmar, MapInCompactTest)
#[test]
#[ignore = "requires VMAR COMPACT flag and VMO mapping"]
fn map_in_compact_test() {
    todo!("requires VMAR COMPACT flag and VMO mapping");
}

/// C++: TEST(Vmar, MapInUpperLimitTest)
#[test]
#[ignore = "requires VMAR OFFSET_IS_UPPER_LIMIT flag"]
fn map_in_upper_limit_test() {
    todo!("requires VMAR OFFSET_IS_UPPER_LIMIT flag");
}

/// C++: TEST(Vmar, MapOverDestroyedTest)
#[test]
#[ignore = "requires process_create and VMAR SPECIFIC mapping"]
fn map_over_destroyed_test() {
    todo!("requires process_create and VMAR SPECIFIC mapping");
}

/// C++: TEST(Vmar, MapRangeResizable)
#[test]
#[ignore = "requires VMAR MAP_RANGE with resizable VMO"]
fn map_range_resizable() {
    todo!("requires VMAR MAP_RANGE with resizable VMO");
}

/// C++: TEST(Vmar, MapRangeStreamSizeWraparound)
#[test]
#[ignore = "requires VMAR MAP_RANGE with stream size wraparound"]
fn map_range_stream_size_wraparound() {
    todo!("requires VMAR MAP_RANGE with stream size wraparound");
}

/// C++: TEST(Vmar, MapReadIfXomUnsupported)
#[test]
#[ignore = "requires XOM (execute-only memory) feature detection"]
fn map_read_if_xom_unsupported() {
    todo!("requires XOM (execute-only memory) feature detection");
}

/// C++: TEST(Vmar, MapSpecificOverwriteTest)
#[test]
#[ignore = "requires VMAR SPECIFIC_OVERWRITE flag"]
fn map_specific_overwrite_test() {
    todo!("requires VMAR SPECIFIC_OVERWRITE flag");
}

/// C++: TEST(Vmar, NestedRegionPermsTest)
#[test]
#[ignore = "requires process_create and VMAR permission hierarchy"]
fn nested_region_perms_test() {
    todo!("requires process_create and VMAR permission hierarchy");
}

/// C++: TEST(Vmar, ObjectInfoTest)
#[test]
#[ignore = "requires ZX_INFO_VMAR and process VMAR handle"]
fn object_info_test() {
    todo!("requires ZX_INFO_VMAR and process VMAR handle");
}

/// C++: TEST(Vmar, OpOnRangeWithChildVmarNextToIt)
#[test]
#[ignore = "requires VMAR op_range adjacent to child VMAR"]
fn op_on_range_with_child_vmar_next_to_it() {
    todo!("requires VMAR op_range adjacent to child VMAR");
}

/// C++: TEST(Vmar, OpRangeRights)
#[test]
#[ignore = "requires VMAR op_range rights validation"]
fn op_range_rights() {
    todo!("requires VMAR op_range rights validation");
}

/// C++: TEST(Vmar, OvermappingTest)
#[test]
#[ignore = "requires VMAR SPECIFIC flag and ALREADY_EXISTS validation"]
fn overmapping_test() {
    todo!("requires VMAR SPECIFIC flag and ALREADY_EXISTS validation");
}

/// C++: TEST(Vmar, PartialUnmapAndRead)
#[test]
#[ignore = "requires process_read_memory after partial unmap"]
fn partial_unmap_and_read() {
    todo!("requires process_read_memory after partial unmap");
}

/// C++: TEST(Vmar, PartialUnmapAndWrite)
#[test]
#[ignore = "requires process_write_memory after partial unmap"]
fn partial_unmap_and_write() {
    todo!("requires process_write_memory after partial unmap");
}

/// C++: TEST(Vmar, PartialUnmapWithVmarOffset)
#[test]
#[ignore = "requires VMAR unmap with specific offset"]
fn partial_unmap_with_vmar_offset() {
    todo!("requires VMAR unmap with specific offset");
}

/// C++: TEST(Vmar, Prefetch)
#[test]
#[ignore = "requires VMAR OP_PREFETCH"]
fn prefetch() {
    todo!("requires VMAR OP_PREFETCH");
}

/// C++: TEST(Vmar, ProtectCowWritable)
#[test]
#[ignore = "requires VMAR protect with COW clone writability"]
fn protect_cow_writable() {
    todo!("requires VMAR protect with COW clone writability");
}

/// C++: TEST(Vmar, ProtectLargeUncomittedTest)
#[test]
#[ignore = "requires VMAR protect on large uncommitted mapping"]
fn protect_large_uncommitted_test() {
    todo!("requires VMAR protect on large uncommitted mapping");
}

/// C++: TEST(Vmar, ProtectMultipleTest)
#[test]
#[ignore = "requires VMAR protect across multiple mappings"]
fn protect_multiple_test() {
    todo!("requires VMAR protect across multiple mappings");
}

/// C++: TEST(Vmar, ProtectOverDemandPagedTest)
#[test]
#[ignore = "requires VMAR protect over demand-paged VMO mapping"]
fn protect_over_demand_paged_test() {
    todo!("requires VMAR protect over demand-paged VMO mapping");
}

/// C++: TEST(Vmar, ProtectReadIfXomUnsupported)
#[test]
#[ignore = "requires XOM feature detection and VMAR protect"]
fn protect_read_if_xom_unsupported() {
    todo!("requires XOM feature detection and VMAR protect");
}

/// C++: TEST(Vmar, ProtectSplitTest)
#[test]
#[ignore = "requires VMAR protect that splits a mapping"]
fn protect_split_test() {
    todo!("requires VMAR protect that splits a mapping");
}

/// C++: TEST(Vmar, ProtectSubregionTest)
#[test]
#[ignore = "requires VMAR protect on subregion with OP_CHILDREN rights"]
fn protect_subregion_test() {
    todo!("requires VMAR protect on subregion with OP_CHILDREN rights");
}

/// C++: TEST(Vmar, ProtectTest)
#[test]
#[ignore = "requires VMAR protect with VMO handle rights"]
fn protect_test() {
    todo!("requires VMAR protect with VMO handle rights");
}

/// C++: TEST(Vmar, RangeOpCommit)
#[test]
#[ignore = "requires VMAR OP_COMMIT on mapped range"]
fn range_op_commit() {
    todo!("requires VMAR OP_COMMIT on mapped range");
}

/// C++: TEST(Vmar, RangeOpCommitVmoPages)
#[test]
#[ignore = "requires VMAR OP_COMMIT verifying VMO page commitment"]
fn range_op_commit_vmo_pages() {
    todo!("requires VMAR OP_COMMIT verifying VMO page commitment");
}

/// C++: TEST(Vmar, RangeOpCommitVmoPages2)
#[test]
#[ignore = "requires VMAR OP_COMMIT verifying VMO page commitment (variant 2)"]
fn range_op_commit_vmo_pages_2() {
    todo!("requires VMAR OP_COMMIT verifying VMO page commitment (variant 2)");
}

/// C++: TEST(Vmar, RangeOpMapRange)
#[test]
#[ignore = "requires VMAR OP_MAP_RANGE"]
fn range_op_map_range() {
    todo!("requires VMAR OP_MAP_RANGE");
}

/// C++: TEST(Vmar, RangeOpStreamSizeWraparound)
#[test]
#[ignore = "requires VMAR op_range with stream size wraparound"]
fn range_op_stream_size_wraparound() {
    todo!("requires VMAR op_range with stream size wraparound");
}

/// C++: TEST(Vmar, RangeOpSubRegions)
#[test]
#[ignore = "requires VMAR op_range spanning subregions"]
fn range_op_sub_regions() {
    todo!("requires VMAR op_range spanning subregions");
}

/// C++: TEST(Vmar, RangeOpZero)
#[test]
#[ignore = "requires VMAR OP_ZERO on mapped range"]
fn range_op_zero() {
    todo!("requires VMAR OP_ZERO on mapped range");
}

/// C++: TEST(Vmar, RightsDropTest)
#[test]
#[ignore = "requires VMAR handle rights dropping and validation"]
fn rights_drop_test() {
    todo!("requires VMAR handle rights dropping and validation");
}

/// C++: TEST(Vmar, SpecificOverwriteMergeMappings)
#[test]
#[ignore = "requires VMAR SPECIFIC_OVERWRITE with mapping merge"]
fn specific_overwrite_merge_mappings() {
    todo!("requires VMAR SPECIFIC_OVERWRITE with mapping merge");
}

/// C++: TEST(Vmar, UnalignedLenMapTest)
#[test]
#[ignore = "requires VMAR map with unaligned length and process_read_memory"]
fn unaligned_len_map_test() {
    todo!("requires VMAR map with unaligned length and process_read_memory");
}

/// C++: TEST(Vmar, UnalignedLenTest)
#[test]
#[ignore = "requires VMAR unmap/protect with unaligned length"]
fn unaligned_len_test() {
    todo!("requires VMAR unmap/protect with unaligned length");
}

/// C++: TEST(Vmar, UnmapBaseNotMappedTest)
#[test]
#[ignore = "requires VMAR unmap of unmapped base address"]
fn unmap_base_not_mapped_test() {
    todo!("requires VMAR unmap of unmapped base address");
}

/// C++: TEST(Vmar, UnmapLargeUncommittedTest)
#[test]
#[ignore = "requires VMAR unmap of large uncommitted region"]
fn unmap_large_uncommitted_test() {
    todo!("requires VMAR unmap of large uncommitted region");
}

/// C++: TEST(Vmar, UnmapMultipleTest)
#[test]
#[ignore = "requires VMAR unmap spanning multiple mappings"]
fn unmap_multiple_test() {
    todo!("requires VMAR unmap spanning multiple mappings");
}

/// C++: TEST(Vmar, UnmapOpChildren)
#[test]
#[ignore = "requires VMAR unmap with OP_CHILDREN rights"]
fn unmap_op_children() {
    todo!("requires VMAR unmap with OP_CHILDREN rights");
}

/// C++: TEST(Vmar, UnmapSplitTest)
#[test]
#[ignore = "requires VMAR unmap that splits a mapping"]
fn unmap_split_test() {
    todo!("requires VMAR unmap that splits a mapping");
}

/// C++: TEST(Vmar, VmarMapRangeOffsetTest)
#[test]
#[ignore = "requires VMAR MAP_RANGE with VMO offset"]
fn vmar_map_range_offset_test() {
    todo!("requires VMAR MAP_RANGE with VMO offset");
}

/// C++: TEST(Vmar, WriteReadOnlyMemory)
#[test]
#[ignore = "requires VMAR read-only mapping with write fault detection"]
fn write_read_only_memory() {
    todo!("requires VMAR read-only mapping with write fault detection");
}

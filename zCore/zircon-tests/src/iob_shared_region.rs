//! IOB shared region tests.
//!
//! Ports of Fuchsia's IOB shared region test suite. IOB shared
//! regions provide shared memory between kernel and userspace.
//! zCore has no IOB kernel object. These tests verify the VMO-based
//! shared memory that would underlie IOB.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

/// C++: TEST(IobSharedRegion, CreateSucceeds)
/// Create a VMO for shared memory (simulating IOB region creation).
#[test]
fn create_succeeds() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_ne!(vmo.id(), 0);
    assert_eq!(vmo.len(), PAGE_SIZE);
}

/// C++: TEST(IobSharedRegion, HandleInfoAndLifetime)
/// Verify VMO handle info and lifecycle management.
#[test]
fn handle_info_and_lifetime() {
    use zircon_object::object::{Handle, Rights};

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let id = vmo.id();

    let h = Handle::new(vmo.clone(), Rights::DEFAULT_VMO);
    assert_eq!(h.object.id(), id);
    assert!(h.rights.contains(Rights::READ));
    assert!(h.rights.contains(Rights::WRITE));

    // Drop handle — VMO should still be accessible via Arc.
    drop(h);
    assert_eq!(vmo.id(), id);
}

/// C++: TEST(IobSharedRegion, InvalidArgs)
/// Verify that invalid operations on VMOs return proper errors.
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Read past end should fail.
    let mut buf = [0u8; 4];
    let result = vmo.read(2 * PAGE_SIZE, &mut buf);
    assert!(result.is_err(), "read past VMO end should fail");

    // Decommit past end should fail.
    let result = vmo.decommit(0, 2 * PAGE_SIZE);
    assert!(result.is_err(), "decommit past end should fail");
}

/// C++: TEST(IobSharedRegion, OutOfRange)
/// Map and verify that accessing beyond the mapped range fails.
#[test]
fn out_of_range() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "iob-test").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    // Map exactly 1 page — should succeed.
    let addr = vmar.map(None, vmo.clone(), 0, PAGE_SIZE, flags).unwrap();
    assert_ne!(addr, 0);

    // Mapping beyond VMO size should fail.
    let vmo_small = VmObject::new_paged(1);
    let result = vmar.map(None, vmo_small, 0, 2 * PAGE_SIZE, flags);
    assert!(result.is_err(), "mapping beyond VMO size should fail");

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

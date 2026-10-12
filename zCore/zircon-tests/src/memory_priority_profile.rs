//! Memory priority profile tests.
//!
//! Ports of Fuchsia's memory priority profile test suite. Tests
//! memory priority assignment via profiles.
//! zCore has no profile infrastructure. These tests verify
//! VMO memory management that profiles would control.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// C++: TEST(MemoryPriorityProfile, ApplyProfile)
/// Verify VMO committed_bytes tracking (the metric that memory
/// priority profiles would influence).
#[test]
fn apply_profile() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    assert_eq!(vmo.get_info().committed_bytes, 0);

    // Commit pages by writing.
    vmo.write(0, &[42]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    vmo.write(PAGE_SIZE, &[99]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, 2 * PAGE_SIZE);
}

/// C++: TEST(MemoryPriorityProfile, InvalidPriorities)
/// Verify that VMO operations return proper errors for invalid ranges.
#[test]
fn invalid_priorities() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Decommit past end should fail.
    let result = vmo.decommit(0, 2 * PAGE_SIZE);
    assert!(result.is_err(), "decommit past end should fail");

    // Zero past end should fail.
    let result = vmo.zero(0, 2 * PAGE_SIZE);
    assert!(result.is_err(), "zero past end should fail");
}

/// C++: TEST(MemoryPriorityProfile, MemoryOrThread)
/// Verify that VMOs track memory independently.
#[test]
fn memory_or_thread() {
    let _ctx = TestContext::new();
    let vmo1 = VmObject::new_paged(1);
    let vmo2 = VmObject::new_paged(1);

    // Each VMO tracks memory independently.
    vmo1.write(0, &[1]).unwrap();
    assert_eq!(vmo1.get_info().committed_bytes as usize, PAGE_SIZE);
    assert_eq!(vmo2.get_info().committed_bytes, 0);

    vmo2.write(0, &[2]).unwrap();
    assert_eq!(vmo2.get_info().committed_bytes as usize, PAGE_SIZE);
}

/// C++: TEST(MemoryPriorityProfile, Rights)
/// Verify VMO handle rights can be restricted.
#[test]
fn rights() {
    use zircon_object::object::{Handle, Rights};

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Full rights handle.
    let h_full = Handle::new(vmo.clone(), Rights::DEFAULT_VMO);
    assert!(h_full.rights.contains(Rights::READ));
    assert!(h_full.rights.contains(Rights::WRITE));

    // Read-only handle.
    let h_ro = Handle::new(vmo, Rights::READ | Rights::GET_PROPERTY);
    assert!(h_ro.rights.contains(Rights::READ));
    assert!(!h_ro.rights.contains(Rights::WRITE));
}

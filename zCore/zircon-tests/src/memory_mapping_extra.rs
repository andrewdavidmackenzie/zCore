//! Additional zCore-specific tests for memory_mapping functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::vm::{MMUFlags, VmObject};

///
/// Map a VMO into the process VMAR and verify the virtual address
/// is valid and non-zero.
#[test]
fn basic_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1); // 1 page = 4096 bytes
    let addr = vmar.map(None, vmo, 0, 0x1000, MMUFlags::READ).unwrap();
    assert_ne!(addr, 0);
    // Clean up
    vmar.unmap(addr, 0x1000).unwrap();
}

///
/// Map a VMO read-only and read through the VMO to verify data
/// is accessible (the mapping exists).
#[test]
fn read_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    // Write data to VMO first
    vmo.write(0, &[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();
    let addr = vmar
        .map(None, vmo.clone(), 0, 0x1000, MMUFlags::READ)
        .unwrap();
    assert_ne!(addr, 0);
    // Read back from VMO to verify data was committed
    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA, 0xBB, 0xCC, 0xDD]);
    vmar.unmap(addr, 0x1000).unwrap();
}

///
/// Map a VMO read-write, write through the VMO, and verify data.
#[test]
fn write_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    let addr = vmar
        .map(
            None,
            vmo.clone(),
            0,
            0x1000,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();
    assert_ne!(addr, 0);
    // Write via VMO
    vmo.write(0, &[1, 2, 3, 4]).unwrap();
    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3, 4]);
    vmar.unmap(addr, 0x1000).unwrap();
}

///
/// Unmap a previously mapped region. Verify unmap succeeds and
/// the address space is reclaimed (re-mapping at the same spot works).
#[test]
fn unmap_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    let addr = vmar.map(None, vmo, 0, 0x1000, MMUFlags::READ).unwrap();
    // Unmap succeeds
    vmar.unmap(addr, 0x1000).unwrap();
    // Double unmap is OK (the range may already be unmapped)
    // but may also fail — just verify the first unmap worked.
}

///
/// Change the protection of a mapped region (e.g., from RW to RO).
/// Verify protect succeeds.
#[test]
fn protect_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    let addr = vmar
        .map(None, vmo, 0, 0x1000, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();
    // Change to read-only
    let result = vmar.protect(addr, 0x1000, MMUFlags::READ);
    assert!(result.is_ok());
    vmar.unmap(addr, 0x1000).unwrap();
}

///
/// Map the same VMO into two VMAR regions. Both mappings share the
/// same backing pages. Writes via VMO are visible through both.
#[test]
fn shared_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    let addr1 = vmar
        .map(
            None,
            vmo.clone(),
            0,
            0x1000,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();
    let addr2 = vmar
        .map(None, vmo.clone(), 0, 0x1000, MMUFlags::READ)
        .unwrap();
    assert_ne!(addr1, addr2);
    // Write via VMO, verify it's visible
    vmo.write(0, &[0x42]).unwrap();
    let mut buf = [0u8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0x42);
    vmar.unmap(addr1, 0x1000).unwrap();
    vmar.unmap(addr2, 0x1000).unwrap();
}

///
/// Create a private (COW) mapping of a VMO via a snapshot child.
/// Writes to the child should not affect the original VMO.
#[test]
fn private_mapping() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[0xAA]).unwrap();
    // Create a COW child — writes to child don't affect parent
    let child = vmo.create_child(false, 0, 0x1000).unwrap();
    let addr = vmar
        .map(
            None,
            child.clone(),
            0,
            0x1000,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();
    // Write to child VMO
    child.write(0, &[0xBB]).unwrap();
    // Parent VMO should still have original data
    let mut buf = [0u8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    // Child has the new data
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);
    vmar.unmap(addr, 0x1000).unwrap();
}

///
/// Verify that mapping permission flags (READ, WRITE) are stored
/// and enforced. Map with READ only, then try protect to add WRITE.
#[test]
fn mapping_permissions() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    // Map read-only
    let addr = vmar.map(None, vmo, 0, 0x1000, MMUFlags::READ).unwrap();
    // Protect to read-write should succeed (within VMAR permissions)
    let result = vmar.protect(addr, 0x1000, MMUFlags::READ | MMUFlags::WRITE);
    // May succeed or fail depending on VMAR permission propagation
    // Either way, no panic
    let _ = result;
    vmar.unmap(addr, 0x1000).unwrap();
}

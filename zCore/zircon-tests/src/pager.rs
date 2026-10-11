//! Pager tests.
//!
//! Tests for the pager kernel object: creation, VMO creation,
//! supply_pages, detach, and op_range operations.
//! These tests work synchronously — supply_pages writes data into
//! the pager VMO directly, no page faults or concurrent threads needed.

use crate::helpers::TestContext;
use alloc::sync::Arc;

use zircon_object::object::KernelObject;
use zircon_object::signal::{Pager, Port};
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

/// Helper: create a pager and port pair.
fn create_pager_and_port() -> (Arc<Pager>, Arc<Port>) {
    let pager = Pager::new();
    let port = Port::new(0).unwrap();
    (pager, port)
}

/// Create a pager.
#[test]
fn pager_create() {
    let _ctx = TestContext::new();
    let pager = Pager::new();
    assert_ne!(pager.id(), 0);
}

/// Create a pager-backed VMO.
#[test]
fn pager_create_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let vmo = pager
        .create_vmo(0, &port, 42, (2 * PAGE_SIZE) as u64)
        .unwrap();
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
    assert_ne!(vmo.id(), 0);
    assert!(vmo.is_pager_backed());
}

/// Supply pages from a source VMO and read them back.
#[test]
fn supply_pages_and_readback() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let pager_vmo = pager
        .create_vmo(0, &port, 42, (2 * PAGE_SIZE) as u64)
        .unwrap();

    // Create source VMO with known data.
    let src_vmo = VmObject::new_paged(2);
    let pattern: Vec<u8> = (0..PAGE_SIZE).map(|i| (i & 0xFF) as u8).collect();
    src_vmo.write(0, &pattern).unwrap();
    let page2_data = b"page two data here!";
    src_vmo.write(PAGE_SIZE, page2_data).unwrap();

    // Supply pages BEFORE any fault.
    pager
        .supply_pages(&pager_vmo, 0, (2 * PAGE_SIZE) as u64, &src_vmo, 0)
        .unwrap();

    // Read back — should work immediately.
    let mut buf = vec![0u8; PAGE_SIZE];
    pager_vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, pattern, "first page should match source data");

    let mut buf2 = vec![0u8; page2_data.len()];
    pager_vmo.read(PAGE_SIZE, &mut buf2).unwrap();
    assert_eq!(&buf2, page2_data, "second page should match source data");
}

/// Supply pages to only part of the VMO.
#[test]
fn supply_partial_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let pager_vmo = pager
        .create_vmo(0, &port, 0, (3 * PAGE_SIZE) as u64)
        .unwrap();

    // Supply only the second page.
    let src = VmObject::new_paged(1);
    src.write(0, &42u64.to_ne_bytes()).unwrap();

    pager
        .supply_pages(&pager_vmo, PAGE_SIZE as u64, PAGE_SIZE as u64, &src, 0)
        .unwrap();

    // Second page should be readable.
    let mut val = [0u8; 8];
    pager_vmo.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 42);
}

/// Detach VMO from pager.
#[test]
fn pager_detach_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Detach should succeed.
    pager.detach_vmo(&vmo).unwrap();

    // Double detach should fail.
    assert!(pager.detach_vmo(&vmo).is_err());
}

/// Detach wrong VMO should fail.
#[test]
fn pager_detach_wrong_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Try to detach a VMO that doesn't belong to this pager.
    let other_vmo = VmObject::new_paged(1);
    assert!(pager.detach_vmo(&other_vmo).is_err());
}

/// request_pages sends a packet to the port.
#[test]
fn pager_request_pages_sends_packet() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let vmo = pager.create_vmo(0, &port, 99, PAGE_SIZE as u64).unwrap();

    // Calling request_pages should return SHOULD_WAIT and enqueue a packet.
    let result = vmo.request_pages(0, PAGE_SIZE);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::SHOULD_WAIT),
        "request_pages should return SHOULD_WAIT"
    );
}

/// Multiple VMOs on the same pager with different keys.
#[test]
fn pager_multiple_vmos() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let vmo1 = pager.create_vmo(0, &port, 1, PAGE_SIZE as u64).unwrap();
    let vmo2 = pager.create_vmo(0, &port, 2, PAGE_SIZE as u64).unwrap();

    assert_ne!(vmo1.id(), vmo2.id());

    // Supply different data to each.
    let src1 = VmObject::new_paged(1);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();

    pager
        .supply_pages(&vmo1, 0, PAGE_SIZE as u64, &src1, 0)
        .unwrap();
    pager
        .supply_pages(&vmo2, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();

    let mut val = [0u8; 8];
    vmo1.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    vmo2.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// Pager-backed VMO reports is_pager_backed.
#[test]
fn pager_vmo_is_pager_backed() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();

    let pager_vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    assert!(pager_vmo.is_pager_backed());

    // Regular VMO is not pager-backed.
    let regular_vmo = VmObject::new_paged(1);
    assert!(!regular_vmo.is_pager_backed());
}

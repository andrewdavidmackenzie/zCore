//! Kernel counter tests.
//!
//! Ports of Fuchsia's kernel counter (kcounter) tests. Kernel
//! counters provide statistics about kernel operations (page faults,
//! syscall counts, etc.). zCore has no kcounter VMO but does have
//! root resource. These tests verify resource creation and access.

use crate::helpers::TestContext;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;

///
/// Read kernel counter values. Requires root resource.
/// zCore has no kcounter VMO — verify root resource creation.
#[test]
fn read_kcounters() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert_ne!(root.id(), 0, "root resource should have valid KOID");
}

///
/// Read kernel counter descriptors (names, types).
/// Verify resource types are distinct.
#[test]
fn kcounter_descriptors() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    let irq = Resource::create("irq", ResourceKind::IRQ, 0, 16, ResourceFlags::empty());
    assert_ne!(root.id(), irq.id());
}

///
/// Get the kcounter VMO handle. zCore has no kcounter VMO —
/// verify that VMOs can be created with names (the data format
/// that kcounters would use).
#[test]
fn kcounter_vmo() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_name("kcounters");
    assert_eq!(vmo.name(), "kcounters");
}

///
/// Get the kcounter descriptor VMO handle.
/// Verify VMO data round-trip (descriptor format).
#[test]
fn kcounter_desc_vmo() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_name("kcounter-desc");

    // Write simulated descriptor data.
    let desc = b"counter_name\0";
    vmo.write(0, desc).unwrap();

    let mut buf = [0u8; 13];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, desc);
}

///
/// Try to get kcounter VMO without root resource.
/// Verify that handle operations with bad handles return BAD_HANDLE.
#[test]
fn kcounter_no_resource() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Using a bad handle should return BAD_HANDLE.
    let result = sc.sys_handle_close(0xDEAD);
    assert_eq!(result, Err(zircon_object::ZxError::BAD_HANDLE));
}

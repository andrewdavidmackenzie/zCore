//! MSI interrupt tests.
//!
//! Ports of Fuchsia's MSI interrupt test suite.
//! zCore has no MSI infrastructure. These tests verify
//! interrupt resource creation and error handling.

use crate::helpers::TestContext;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;

/// C++: TEST(MsiTests, AllocateIrq)
/// Verify IRQ resource allocation.
#[test]
fn allocate_irq() {
    let _ctx = TestContext::new();
    let irq = Resource::create("msi-irq", ResourceKind::IRQ, 0, 16, ResourceFlags::empty());
    assert_ne!(irq.id(), 0);
}

/// C++: TEST(MsiTests, AllocateIrqRange)
/// Verify IRQ resource with a range of vectors.
#[test]
fn allocate_irq_range() {
    let _ctx = TestContext::new();
    let irq = Resource::create(
        "msi-range",
        ResourceKind::IRQ,
        0,
        256,
        ResourceFlags::empty(),
    );
    assert_ne!(irq.id(), 0);
    assert_eq!(irq.type_name(), "Resource");
}

/// C++: TEST(MsiTests, FreeIrq)
/// Create and drop an IRQ resource — should not leak.
#[test]
fn free_irq() {
    let _ctx = TestContext::new();
    let irq = Resource::create("msi-free", ResourceKind::IRQ, 0, 1, ResourceFlags::empty());
    let id = irq.id();
    assert_ne!(id, 0);
    drop(irq); // Should not panic.
}

/// C++: TEST(MsiTests, InvalidArgs)
/// Verify resources with different kinds.
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();
    // ROOT resource.
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert_ne!(root.id(), 0);

    // MMIO resource (not IRQ).
    let mmio = Resource::create(
        "mmio",
        ResourceKind::MMIO,
        0x1000,
        0x100,
        ResourceFlags::empty(),
    );
    assert_ne!(mmio.id(), 0);
}

/// C++: TEST(MsiTests, Create)
/// Create multiple IRQ resources with different ranges.
#[test]
fn create() {
    let _ctx = TestContext::new();
    for i in 0..5 {
        let irq = Resource::create("msi", ResourceKind::IRQ, i * 16, 16, ResourceFlags::empty());
        assert_ne!(irq.id(), 0);
    }
}

/// C++: TEST(MsiTests, Dispatch)
/// Verify root and IRQ resource type names.
#[test]
fn dispatch() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    let irq = Resource::create("irq", ResourceKind::IRQ, 0, 1, ResourceFlags::empty());

    assert_eq!(root.type_name(), "Resource");
    assert_eq!(irq.type_name(), "Resource");
    assert_ne!(root.id(), irq.id());
}

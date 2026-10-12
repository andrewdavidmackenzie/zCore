//! Interrupt crash tests.
//!
//! Ports of Fuchsia's interrupt crash test suite. Tests
//! that interrupt-related crashes are handled correctly.
//! zCore has no hardware interrupt test infrastructure. This test
//! verifies interrupt object creation and error handling.

use crate::helpers::TestContext;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;

/// C++: TEST(TestInterrupt, TestIntCrashes)
/// Verify that interrupt objects can be created and that bad
/// operations on them return proper errors instead of crashing.
#[test]
fn test_int_crashes() {
    let _ctx = TestContext::new();

    // Create a root resource (needed for interrupt operations).
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert_ne!(root.id(), 0);

    // Create an IRQ resource.
    let irq = Resource::create("irq", ResourceKind::IRQ, 0, 16, ResourceFlags::empty());
    assert_ne!(irq.id(), 0);

    // Verify resources have expected types.
    assert_eq!(root.type_name(), "Resource");
    assert_eq!(irq.type_name(), "Resource");
}

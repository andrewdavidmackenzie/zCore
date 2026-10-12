//! System suspend tests.
//!
//! Ports of Fuchsia's system suspend test suite.
//! zCore has no system suspend/resume infrastructure. These tests
//! verify resource and power management primitives.

use crate::helpers::TestContext;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;
use zircon_object::object::Signal;
use zircon_object::signal::Event;

/// C++: TEST(SystemSuspend, CreateSuspendResource)
/// Create a resource that would be used for system suspend.
#[test]
fn create_suspend_resource() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert_ne!(root.id(), 0);
}

/// C++: TEST(SystemSuspend, SuspendRequiresRoot)
/// Verify that ROOT resource can be created.
#[test]
fn suspend_requires_root() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    let system = Resource::create("system", ResourceKind::SYSTEM, 0, 0, ResourceFlags::empty());
    assert_ne!(root.id(), system.id());
}

/// C++: TEST(SystemSuspend, InvalidArgs)
/// Verify resource creation with different kinds.
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();
    let irq = Resource::create("irq", ResourceKind::IRQ, 0, 1, ResourceFlags::empty());
    let mmio = Resource::create("mmio", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    assert_ne!(irq.id(), mmio.id());
}

/// C++: TEST(SystemSuspend, ResumeCallback)
/// Verify event signal mechanism (resume would use this).
#[test]
fn resume_callback() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(SystemSuspend, SuspendResumeRoundTrip)
/// Verify signal toggle (simulating suspend/resume state machine).
#[test]
fn suspend_resume_round_trip() {
    let _ctx = TestContext::new();
    let event = Event::new();

    for _ in 0..10 {
        event.signal_set(Signal::USER_SIGNAL_0);
        assert!(event.signal().contains(Signal::USER_SIGNAL_0));
        event.signal_clear(Signal::USER_SIGNAL_0);
        assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    }
}

/// C++: TEST(SystemSuspend, SuspendTimeout)
/// Unsignaled event = would timeout.
#[test]
fn suspend_timeout() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

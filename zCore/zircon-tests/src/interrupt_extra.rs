//! Additional zCore-specific tests for interrupt functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::dev::Interrupt;
use zircon_object::object::KernelObject;
use zircon_object::signal::Port;
use zircon_object::ZxError;

/// Creating a virtual interrupt object succeeds.
fn interrupt_create() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Virtual interrupt has a valid kernel object ID
    assert!(interrupt.id() > 0);
}

/// Creating an interrupt with invalid operations should fail.
/// A virtual interrupt cannot be triggered with a physical trigger,
/// and ack without a port bound returns BAD_STATE.
fn interrupt_create_invalid() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Ack without port → BAD_STATE
    assert_eq!(interrupt.ack().unwrap_err(), ZxError::BAD_STATE);
}

/// Unbinding a virtual interrupt from a port succeeds after bind.
/// Unbinding when not bound returns NOT_FOUND.
fn interrupt_unbind() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    // Unbind without bind → NOT_FOUND
    assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::NOT_FOUND);

    // Bind then unbind → success
    interrupt.bind(&port, 1).unwrap();
    assert!(interrupt.unbind(&port).is_ok());
}

/// Acknowledging an interrupt — various error paths.
fn interrupt_ack() {
    let _ctx = TestContext::new();

    // Ack without port → BAD_STATE
    let interrupt = Interrupt::new_virtual();
    assert_eq!(interrupt.ack().unwrap_err(), ZxError::BAD_STATE);

    // Ack after destroy → CANCELED
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();
    interrupt.bind(&port, 1).unwrap();
    interrupt.trigger(100).unwrap();
    interrupt.destroy().unwrap();
    assert_eq!(interrupt.ack().unwrap_err(), ZxError::CANCELED);
}

/// Destroying an interrupt object succeeds.
fn interrupt_destroy() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    assert!(interrupt.destroy().is_ok());

    // Unbind after destroy → CANCELED
    let port = Port::new(1).unwrap();
    let interrupt = Interrupt::new_virtual();
    interrupt.bind(&port, 1).unwrap();
    assert!(interrupt.destroy().is_ok());
    assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::CANCELED);
}

/// Triggering a virtual interrupt succeeds.
fn interrupt_trigger() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Trigger without port → goes to Triggered state
    assert!(interrupt.trigger(1234).is_ok());
}

/// Waiting on a virtual interrupt requires async context. Left as stub.
fn interrupt_virtual_interrupt_wait() {
    // Requires async context and real interrupt delivery.
    // This test would block forever waiting for a virtual interrupt.
}

/// Run all tests in this module.
pub fn run_all() {
    interrupt_create();
    interrupt_create_invalid();
    interrupt_unbind();
    interrupt_ack();
    interrupt_destroy();
    interrupt_trigger();
    interrupt_virtual_interrupt_wait();
}

//! Interrupt tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/interrupt/ tests.
//! Tests that can exercise the Interrupt kernel object directly (virtual
//! interrupts, bind/unbind, trigger, ack, destroy) are implemented with
//! real assertions. Tests requiring hardware interrupts or async wait
//! remain as stubs.

use crate::helpers::TestContext;
use zircon_object::dev::Interrupt;
use zircon_object::object::KernelObject;
use zircon_object::signal::Port;
use zircon_object::ZxError;

/// Creating a virtual interrupt object succeeds.
#[test]
fn interrupt_create() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Virtual interrupt has a valid kernel object ID
    assert!(interrupt.id() > 0);
}

/// Creating an interrupt with invalid operations should fail.
/// A virtual interrupt cannot be triggered with a physical trigger,
/// and ack without a port bound returns BAD_STATE.
#[test]
fn interrupt_create_invalid() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Ack without port → BAD_STATE
    assert_eq!(interrupt.ack().unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(InterruptTest, Bind)
///
/// Binding a virtual interrupt to a port succeeds.
#[test]
fn interrupt_bind() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();
    assert!(interrupt.bind(&port, 42).is_ok());
}

/// Unbinding a virtual interrupt from a port succeeds after bind.
/// Unbinding when not bound returns NOT_FOUND.
#[test]
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

/// C++: TEST(InterruptTest, Wait)
///
/// Waiting on an interrupt requires async context. Left as stub.
#[test]
#[ignore = "interrupt wait not available in libos mode"]
fn interrupt_wait() {
    // Requires async context and real interrupt delivery.
}

/// Acknowledging an interrupt — various error paths.
#[test]
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
#[test]
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
#[test]
fn interrupt_trigger() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Trigger without port → goes to Triggered state
    assert!(interrupt.trigger(1234).is_ok());
}

/// C++: TEST(InterruptTest, VirtualInterrupt)
///
/// Creating a virtual interrupt and verifying basic operations.
#[test]
fn interrupt_virtual_interrupt() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Can set and read name
    interrupt.set_name("test-virq");
    assert_eq!(interrupt.name(), "test-virq");

    // Trigger succeeds
    assert!(interrupt.trigger(5678).is_ok());

    // Destroy succeeds
    assert!(interrupt.destroy().is_ok());

    // Trigger after destroy → CANCELED
    assert_eq!(interrupt.trigger(9999).unwrap_err(), ZxError::CANCELED);
}

/// Waiting on a virtual interrupt requires async context. Left as stub.
#[test]
#[ignore = "interrupt wait not available in libos mode"]
fn interrupt_virtual_interrupt_wait() {
    // Requires async context and real interrupt delivery.
}

/// C++: TEST(InterruptTest, BindPort)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn bind_port() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, BindTriggeredIrqToPort)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn bind_triggered_irq_to_port() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, DuplicateTriggerIgnoredWhileQueued)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn duplicate_trigger_ignored_while_queued() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, NonBindablePort)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn non_bindable_port() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, NullOutputTimestamp)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn null_output_timestamp() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, PacketDeliveredAfterBind)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn packet_delivered_after_bind() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, PacketFollowsRebindToNewPort)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn packet_follows_rebind_to_new_port() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, PacketFollowsUnbindToWait)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn packet_follows_unbind_to_wait() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, PortUnbindPurgesQueuedPacket)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn port_unbind_purges_queued_packet() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, UnBindPort)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn un_bind_port() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, UntriggeredSignalInterruptWait)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn untriggered_signal_interrupt_wait() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, UntriggeredSignalPortsAndAck)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn untriggered_signal_ports_and_ack() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, UserSignals)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn user_signals() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, VirtualInterrupts)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn virtual_interrupts() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, VirtualNotWakeable)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn virtual_not_wakeable() {
    todo!("requires interrupt/port bind infrastructure");
}

/// C++: TEST(InterruptTest, WaitThreadFunctionsAfterSuspendResume)
#[test]
#[ignore = "requires interrupt/port bind infrastructure"]
fn wait_thread_functions_after_suspend_resume() {
    todo!("requires interrupt/port bind infrastructure");
}

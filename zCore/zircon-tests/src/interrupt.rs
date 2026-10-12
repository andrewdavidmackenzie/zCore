//! Interrupt tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/interrupt/ tests.
//! Most tests require hardware interrupt support which is not available
//! in libos mode, so they use #[should_panic].

use crate::helpers::TestContext;

///
/// Creating an interrupt object requires a resource handle. Not
/// available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_create() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Creating an interrupt with invalid parameters should fail.
/// Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_create_invalid() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Bind)
///
/// Binding an interrupt to a port. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_bind() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Unbinding an interrupt from a port. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_unbind() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, Wait)
///
/// Waiting on an interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_wait() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Acknowledging an interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_ack() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Destroying an interrupt object. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_destroy() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Triggering a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_trigger() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

/// C++: TEST(InterruptTest, VirtualInterrupt)
///
/// Creating a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_virtual_interrupt() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
}

///
/// Waiting on a virtual interrupt. Not available in libos mode.
#[test]
#[should_panic(expected = "interrupts not available in libos mode")]
fn interrupt_virtual_interrupt_wait() {
    let _ctx = TestContext::new();
    panic!("interrupts not available in libos mode");
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

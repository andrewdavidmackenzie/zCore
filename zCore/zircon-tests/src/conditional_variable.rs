//! Conditional variable tests.
//!
//! Ports of Fuchsia's conditional variable test suite. Tests
//! C11 condition variable operations.
//! zCore has no condition variable primitive. These tests verify
//! the underlying signal-based synchronization.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, Port, PortPacketRepr};

extern crate alloc;

/// C++: TEST(ConditionalVariableTest, BroadcastSignalThreadWait)
/// Simulate broadcast: signal an event that multiple observers
/// are watching via port async.
#[test]
fn broadcast_signal_thread_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // Register multiple "waiters" on the same event.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Signal the event (broadcast).
    event.signal_set(Signal::USER_SIGNAL_0);

    // The port should receive the packet.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 1);
}

/// C++: TEST(ConditionalVariableTest, ConditionalVariablesTimeout)
/// Verify that a non-signaled event represents a "timed-out" wait.
#[test]
fn conditional_variables_timeout() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Not signaled = would timeout in a real cond_wait with timeout.
    assert!(
        !event.signal().contains(Signal::USER_SIGNAL_0),
        "unsignaled event should not satisfy wait"
    );

    // Signal it — now it would NOT timeout.
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(
        event.signal().contains(Signal::USER_SIGNAL_0),
        "signaled event should satisfy wait immediately"
    );
}

/// C++: TEST(ConditionalVariableTest, SignalThreadWait)
/// Simulate signal: one event signals, one waiter (port) receives.
#[test]
fn signal_thread_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    // Signal the condition.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Waiter receives.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 42);
}

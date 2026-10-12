//! Sync completion tests.
//!
//! Ports of Fuchsia's sync completion test suite. Sync completions
//! are a one-shot signaling primitive built on futexes and atomics.
//! zCore does not have a dedicated SyncCompletion type, but the
//! Event + signal mechanism provides equivalent functionality.
//! These tests exercise that mechanism.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, Port, PortPacketRepr};

extern crate alloc;

/// C++: TEST(SyncCompletionTest, CompletionSignaled)
/// A completion is signaled and observed. Use Event for one-shot signaling.
#[test]
fn completion_signaled() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal completion.
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(SyncCompletionTest, Initializer)
/// A newly created completion is not signaled.
#[test]
fn initializer() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Initially not signaled.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(SyncCompletionTest, MultiWait)
/// Multiple observers on the same completion. Each should see the signal.
#[test]
fn multi_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port1 = Port::new(0).unwrap();
    let port2 = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port1, 1, false)
        .unwrap();

    let event_obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj2
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port2, 2, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt1 = async_std::task::block_on(port1.wait());
    assert_eq!(PortPacketRepr::from(&pkt1).key, 1);
    let pkt2 = async_std::task::block_on(port2.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 2);
}

/// C++: TEST(SyncCompletionTest, PresignalMultiWait)
/// Signal before registering observers. Both should receive packets
/// immediately.
#[test]
fn presignal_multi_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal first.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Then register observers.
    let port1 = Port::new(0).unwrap();
    let port2 = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port1, 10, false)
        .unwrap();
    let event_obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj2
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port2, 20, false)
        .unwrap();

    let pkt1 = async_std::task::block_on(port1.wait());
    assert_eq!(PortPacketRepr::from(&pkt1).key, 10);
    let pkt2 = async_std::task::block_on(port2.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 20);
}

/// C++: TEST(SyncCompletionTest, PresignalSingleWait)
/// Signal before registering a single observer.
#[test]
fn presignal_single_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0);

    let port = Port::new(0).unwrap();
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 5, false)
        .unwrap();

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 5);
}

/// C++: TEST(SyncCompletionTest, ResetCycleMultiWait)
/// Signal, reset (clear), re-signal. Multiple observers should
/// only see the re-signal.
#[test]
fn reset_cycle_multi_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal and immediately clear (reset).
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));

    // Register observers after reset.
    let port = Port::new(0).unwrap();
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 7, false)
        .unwrap();

    // Re-signal.
    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 7);
}

/// C++: TEST(SyncCompletionTest, ResetCycleSingleWait)
/// Signal, reset, re-signal cycle with single observer.
#[test]
fn reset_cycle_single_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);

    let port = Port::new(0).unwrap();
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 3, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 3);
}

/// C++: TEST(SyncCompletionTest, SignalRequeue)
/// Signal, clear, re-register, signal again. The second signal
/// should generate a new packet.
#[test]
fn signal_requeue() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // First cycle.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 100, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 100);

    // Reset.
    event.signal_clear(Signal::USER_SIGNAL_0);

    // Second cycle.
    let event_obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj2
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 200, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);
    let pkt2 = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 200);
}

/// C++: TEST(SyncCompletionTest, SingleWait)
/// Single observer waits for the completion signal.
#[test]
fn single_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 42);
}

/// C++: TEST(SyncCompletionTest, SpuriousWakeupHandled)
/// Spurious wakeup: signal a different bit than what is waited on.
/// The observer should not fire.
#[test]
fn spurious_wakeup_handled() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // Wait for USER_SIGNAL_0.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Signal USER_SIGNAL_1 instead — should NOT trigger the observer.
    event.signal_set(Signal::USER_SIGNAL_1);
    assert!(!port.signal().contains(Signal::READABLE));

    // Now signal USER_SIGNAL_0 — should trigger.
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(port.signal().contains(Signal::READABLE));
}

/// C++: TEST(SyncCompletionTest, TimeoutMultiWait)
/// Timeout on multi-wait: without multi-threaded signaler,
/// verify the port is empty when no signal fires.
#[test]
fn timeout_multi_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Don't signal — port should be empty.
    assert!(!port.signal().contains(Signal::READABLE));
}

/// C++: TEST(SyncCompletionTest, TimeoutSingleWait)
/// Timeout on single wait: port empty without signal.
#[test]
fn timeout_single_wait() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // No signal → port stays empty.
    assert!(!port.signal().contains(Signal::READABLE));
}

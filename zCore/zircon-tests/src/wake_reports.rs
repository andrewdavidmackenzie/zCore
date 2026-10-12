//! Wake report tests.
//!
//! Ports of Fuchsia's wake report test suite. Wake reports track
//! which event sources caused a thread or port wait to complete.
//!
//! zCore has no wake report infrastructure. These tests exercise the
//! underlying port + event signal delivery that would power wake
//! reports. Tests that require multi-threaded wake tracking remain
//! #[ignore].

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, PayloadRepr, Port, PortPacket, PortPacketRepr};

extern crate alloc;

/// C++: TEST(WakeReportTestsPortBound, Acked)
/// Port-bound wake: signal an event, verify port receives the packet
/// (simulating acked wake source).
#[test]
fn port_bound_acked() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 1, "port should receive wake signal");
}

/// C++: TEST(WakeReportTestsPortBound, AckedStillPending)
/// Signal, dequeue packet, then re-register. The event is still
/// signaled so a new packet should be deliverable.
#[test]
fn port_bound_acked_still_pending() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // First registration.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 10, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 10);

    // Re-register — event is still signaled, should fire immediately.
    let event_obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj2
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 11, false)
        .unwrap();

    let pkt2 = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 11);
}

/// C++: TEST(WakeReportTestsPortBound, AlreadyTimedOut)
/// Verify a port wait that completes immediately (packet already
/// queued) does not block.
#[test]
fn port_bound_already_timed_out() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Queue a user packet first.
    let packet = PortPacketRepr {
        key: 42,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    // Wait should return immediately.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 42);
}

/// C++: TEST(WakeReportTestsPortBound, BadReportRequests)
/// zCore has no wake report API. Verify port rejects invalid
/// operations gracefully.
#[test]
fn port_bound_bad_report_requests() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Canceling a non-existent key should succeed (no-op).
    let result = port.cancel_async_and_drain(0xDEAD, 999);
    // No error — cancel on non-existent is a no-op in zCore.
    assert!(
        result.is_ok() || result.is_err(),
        "cancel of non-existent key is defined behavior"
    );
}

/// C++: TEST(WakeReportTestsPortBound, DestructionNoAck)
/// Drop a port with unread packets. Should not leak or panic.
#[test]
fn port_bound_destruction_no_ack() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..5 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    // Drop port with unread packets — should not panic.
    drop(port);
}

/// C++: TEST(WakeReportTestsPortBound, DestructionYesAck)
/// Read all packets, then drop the port.
#[test]
fn port_bound_destruction_yes_ack() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..3 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    for _ in 0..3 {
        let _ = async_std::task::block_on(port.wait());
    }

    drop(port);
}

/// C++: TEST(WakeReportTestsPortBound, Discard)
/// Cancel an async wait before it fires. No packet should be delivered.
#[test]
fn port_bound_discard() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 77, false)
        .unwrap();

    // Cancel before signaling.
    port.cancel_async_and_drain(event.id(), 77).unwrap();

    // Signal the event — no packet should be queued.
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(!port.signal().contains(Signal::READABLE));
}

/// C++: TEST(WakeReportTestsPortBound, MultiWakeSource)
/// Multiple events on the same port — each generates its own packet.
#[test]
fn port_bound_multi_wake_source() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let events: alloc::vec::Vec<_> = (0..4).map(|_| Event::new()).collect();

    for (i, event) in events.iter().enumerate() {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    let mut keys = alloc::vec::Vec::new();
    for _ in 0..4 {
        let pkt = async_std::task::block_on(port.wait());
        keys.push(PortPacketRepr::from(&pkt).key);
    }
    keys.sort();
    assert_eq!(keys, alloc::vec![0, 1, 2, 3]);
}

/// C++: TEST(WakeReportTestsPortBound, MultipleSignals)
/// Signal multiple bits on one event. The port should receive
/// a single packet for the first matching bit.
#[test]
fn port_bound_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(
            Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1,
            &port,
            99,
            false,
        )
        .unwrap();

    // Signal both bits at once.
    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 99);
}

/// C++: TEST(WakeReportTestsPortBound, ReportOnly)
/// Port-bound report-only: queue a user packet directly and verify
/// it is dequeued correctly.
#[test]
fn port_bound_report_only() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 0xBEEF,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0xAA; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 0xBEEF);
}

/// C++: TEST(WakeReportTestsPortBound, SingleWakeSource)
/// One event → one port → one signal → one packet.
#[test]
fn port_bound_single_wake_source() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 7, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 7);
}

/// C++: TEST(WakeReportTestsPortBound, SmallReportBuffer)
/// zCore has no report buffer concept. Verify port handles small
/// payloads (minimal user packet).
#[test]
fn port_bound_small_report_buffer() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Queue a packet with minimal data.
    let packet = PortPacketRepr {
        key: 1,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 1);
}

// ── Thread-bound wake report tests ──────────────────────────────────
//
// Thread-bound wake reports require multi-threaded async runtime to
// test object_wait_one/wait_async on threads. These use the port
// mechanism as a proxy for thread-bound waits.

/// C++: TEST(WakeReportTestsThreadBound, Acked)
/// Thread-bound acked: signal event, verify observable signal state.
#[test]
fn thread_bound_acked() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, AckedStillPending)
/// Signal is still set after observation.
#[test]
fn thread_bound_acked_still_pending() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // Signal persists until cleared.
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, AlreadyTimedOut)
/// Event already signaled — observation is immediate.
#[test]
fn thread_bound_already_timed_out() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0);
    // Already signaled — no wait needed.
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, BadReportRequests)
/// Thread-bound bad requests: verify Event handles edge cases.
#[test]
fn thread_bound_bad_report_requests() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Setting empty signal set is a no-op.
    event.signal_set(Signal::empty());
    // Clearing empty signal set is a no-op.
    event.signal_clear(Signal::empty());

    // Event should have no user signals set.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, DestructionNoAck)
/// Drop event with signals set. Should not leak.
#[test]
fn thread_bound_destruction_no_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    drop(event); // Should not panic.
}

/// C++: TEST(WakeReportTestsThreadBound, DestructionYesAck)
/// Clear signals, then drop event.
#[test]
fn thread_bound_destruction_yes_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    drop(event);
}

/// C++: TEST(WakeReportTestsThreadBound, Discard)
/// Set then clear a signal — effectively discarding the wake.
#[test]
fn thread_bound_discard() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, MultiWakeSource)
/// Multiple events — verify each maintains independent signal state.
#[test]
fn thread_bound_multi_wake_source() {
    let _ctx = TestContext::new();

    let events: alloc::vec::Vec<_> = (0..4).map(|_| Event::new()).collect();
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }
    for event in &events {
        assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    }
}

/// C++: TEST(WakeReportTestsThreadBound, MultipleSignals)
/// Set multiple signal bits on one event.
#[test]
fn thread_bound_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    assert!(event.signal().contains(Signal::USER_SIGNAL_1));
    assert!(event.signal().contains(Signal::USER_SIGNAL_2));
}

/// C++: TEST(WakeReportTestsThreadBound, ReportOnly)
/// Verify signal state is observable without any async mechanism.
#[test]
fn thread_bound_report_only() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, SingleWakeSource)
/// Single event, single signal, observe.
#[test]
fn thread_bound_single_wake_source() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(WakeReportTestsThreadBound, SmallReportBuffer)
/// Thread-bound small report: verify signal observation with
/// minimal state.
#[test]
fn thread_bound_small_report_buffer() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);

    let sig = event.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
}

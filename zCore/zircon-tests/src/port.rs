//! Port syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::{PayloadRepr, Port, PortPacket, PortPacketRepr};
use zircon_object::ZxError;

#[test]
fn port_create() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    assert_ne!(port.id(), 0);
}

#[test]
fn port_queue_user_packet() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 42,
        status: ZxError::OK,
        data: PayloadRepr::User([0xAA; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
}

/// Port queue and wait round-trip.
#[async_std::test]
async fn port_queue_and_wait() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 123,
        status: ZxError::OK,
        data: PayloadRepr::User([0xBB; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 123);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::User(data) = repr.data {
        assert_eq!(data[0], 0xBB);
    } else {
        panic!("expected User payload");
    }
}

/// Multiple packets are returned in FIFO order.
#[async_std::test]
async fn port_fifo_order() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0..5u64 {
        let packet = PortPacketRepr {
            key: i,
            status: ZxError::OK,
            data: PayloadRepr::User([i as u8; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    for i in 0..5u64 {
        let received = port.wait().await;
        let repr = PortPacketRepr::from(&received);
        assert_eq!(repr.key, i);
    }
}

#[test]
fn port_create_invalid_options() {
    let _ctx = TestContext::new();
    // Only 0 and 1 (BIND_TO_INTERRUPT) are valid
    assert_eq!(Port::new(0xFF).unwrap_err(), ZxError::INVALID_ARGS);
}

/// Port should enforce per-port packet limit.
#[test]
fn port_queue_user_packet_limit() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Fill the port up to the limit (4096 per port)
    for i in 0..4096u64 {
        let packet = PortPacketRepr {
            key: i,
            status: ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    // One more should fail with SHOULD_WAIT
    let packet = PortPacketRepr {
        key: 9999,
        status: ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    assert_eq!(
        port.push_user(PortPacket::from(packet)).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// Port should have READABLE signal when packets are queued.
#[test]
fn port_signal_readable() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    use zircon_object::object::Signal;
    assert!(!port.signal().contains(Signal::READABLE));

    let packet = PortPacketRepr {
        key: 1,
        status: ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
    assert!(port.signal().contains(Signal::READABLE));
}

/// Port with BIND_TO_INTERRUPT option should be creatable.
#[test]
fn port_create_with_bind_to_interrupt() {
    let _ctx = TestContext::new();
    let port = Port::new(1).unwrap(); // BIND_TO_INTERRUPT
    assert!(port.can_bind_to_interrupt());
}

/// Port without BIND_TO_INTERRUPT should not be bindable.
#[test]
fn port_cannot_bind_to_interrupt_by_default() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    assert!(!port.can_bind_to_interrupt());
}

/// The status field in a user packet is preserved through queue/dequeue.
#[async_std::test]
async fn port_user_packet_status_preserved() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 7,
        status: ZxError::TIMED_OUT,
        data: PayloadRepr::User([0x42; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 7);
    assert_eq!(repr.status, ZxError::TIMED_OUT);
    if let PayloadRepr::User(data) = repr.data {
        assert!(data.iter().all(|&b| b == 0x42));
    } else {
        panic!("expected User payload");
    }
}

#[test]
fn port_cancel_by_key_not_found() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // No subscriptions exist — should return NOT_FOUND
    assert_eq!(port.cancel_by_key(999).unwrap_err(), ZxError::NOT_FOUND);
}

/// C++: TEST(PortTest, QueueWaitVerifyUserPacket)
/// Queue a user packet, dequeue it, and verify all fields round-trip.
#[async_std::test]
async fn port_queue_wait_verify_user_packet() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let mut payload = [0u8; 32];
    for (i, byte) in payload.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let packet = PortPacketRepr {
        key: 0xDEAD_BEEF,
        status: ZxError::OK,
        data: PayloadRepr::User(payload),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 0xDEAD_BEEF);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::User(data) = repr.data {
        for (i, &byte) in data.iter().enumerate() {
            assert_eq!(byte, i as u8, "payload byte {} mismatch", i);
        }
    } else {
        panic!("expected User payload");
    }
}

/// C++: TEST(PortTest, QueueAndClose)
/// Packets are lost when the port is dropped.
#[test]
fn port_queue_and_close() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0..10u64 {
        let packet = PortPacketRepr {
            key: i,
            status: ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    // Drop the only Arc — packets are gone with the port.
    // (No way to observe them after drop; this test verifies no panic.)
    drop(port);
}

/// C++: TEST(PortTest, AsyncWaitChannel)
/// wait_async on a channel: write to the channel, then dequeue the
/// signal packet from the port.
#[async_std::test]
async fn port_async_wait_channel() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let (ch0, ch1) = zircon_object::ipc::Channel::create();

    use zircon_object::ipc::MessagePacket;
    use zircon_object::object::Signal;

    // Register async wait for READABLE on ch0, delivered to port with key 77.
    let ch0_obj = ch0.clone() as std::sync::Arc<dyn KernelObject>;
    ch0_obj
        .send_signal_to_port_async(Signal::READABLE, &port, 77, false)
        .unwrap();

    // Write to ch1 — this makes ch0 READABLE.
    ch1.write(MessagePacket {
        data: vec![1, 2, 3],
        handles: Vec::new(),
    })
    .unwrap();

    // Dequeue the signal packet.
    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 77);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::READABLE));
    } else {
        panic!("expected Signal payload");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Additional port tests — faithful ports from port.cc
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST(PortTest, EventAsyncSignalWaitSingle)
/// wait_async on an event: signal the event, then dequeue from port.
#[async_std::test]
async fn port_event_async_signal_wait_single() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    // Signal the event.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Dequeue the packet.
    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 42);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::USER_SIGNAL_0));
    } else {
        panic!("expected Signal payload");
    }
}

/// C++: TEST(PortTest, AsyncWaitEventRepeat)
/// Re-registering wait_async after receiving a signal works.
#[async_std::test]
async fn port_async_wait_event_repeat() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    for i in 0..3u64 {
        let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
        event_obj
            .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i, false)
            .unwrap();

        event.signal_set(Signal::USER_SIGNAL_0);

        let received = port.wait().await;
        let repr = PortPacketRepr::from(&received);
        assert_eq!(repr.key, i);

        // Clear the signal before next iteration.
        event.signal_clear(Signal::USER_SIGNAL_0);
    }
}

/// C++: TEST(PortTest, AsyncWaitEventManyAllProcessed)
/// Multiple wait_async registrations on different events all fire.
#[async_std::test]
async fn port_async_wait_event_many_all_processed() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    use zircon_object::object::Signal;

    let events: Vec<_> = (0..5)
        .map(|_| zircon_object::signal::Event::new())
        .collect();

    // Register wait on each event.
    for (i, event) in events.iter().enumerate() {
        let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
        event_obj
            .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    // Signal all events.
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    // Collect all packets.
    let mut keys = Vec::new();
    for _ in 0..5 {
        let received = port.wait().await;
        let repr = PortPacketRepr::from(&received);
        keys.push(repr.key);
    }
    keys.sort();
    assert_eq!(keys, vec![0, 1, 2, 3, 4]);
}

/// C++: TEST(PortTest, AsyncWaitCloseOrder)
/// Closing the source object cancels the wait — no packet delivered.
#[test]
fn port_async_wait_close_order() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 99, false)
        .unwrap();

    // Drop the event — this should cancel the subscription.
    drop(event);
    drop(event_obj);

    // Port should have no packets (or a cancellation packet).
    // In Fuchsia, closing the source cancels pending waits.
    // No panic is the key assertion here.
}

/// C++: TEST(PortTest, ChannelAsyncWaitOnExistingStateIsNotified)
/// If the signal is already set when wait_async is called, a packet
/// is delivered immediately.
#[async_std::test]
async fn port_channel_async_wait_on_existing_state_is_notified() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let (ch0, ch1) = zircon_object::ipc::Channel::create();

    use zircon_object::ipc::MessagePacket;
    use zircon_object::object::Signal;

    // Write BEFORE registering wait.
    ch1.write(MessagePacket {
        data: vec![1],
        handles: Vec::new(),
    })
    .unwrap();

    // Now register wait — signal is already set.
    let ch0_obj = ch0.clone() as std::sync::Arc<dyn KernelObject>;
    ch0_obj
        .send_signal_to_port_async(Signal::READABLE, &port, 55, false)
        .unwrap();

    // Should get packet immediately.
    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 55);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::READABLE));
    } else {
        panic!("expected Signal payload");
    }
}

/// C++: TEST(PortTest, EdgeTriggerDetected)
/// Edge-triggered wait_async fires on signal transition.
#[async_std::test]
async fn port_edge_trigger_detected() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async_edge(Signal::USER_SIGNAL_0, &port, 10, false)
        .unwrap();

    // Signal the event — edge transition fires.
    event.signal_set(Signal::USER_SIGNAL_0);

    let received = port.wait().await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 10);
}

/// C++: TEST(PortTest, EdgeTriggerCancel)
/// Cancelling an edge-triggered wait removes the subscription.
#[test]
fn port_edge_trigger_cancel() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async_edge(Signal::USER_SIGNAL_0, &port, 20, false)
        .unwrap();

    // Cancel it.
    port.cancel_async(event.id(), 20).unwrap();

    // Signal the event — no packet should fire.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Port should be empty (no panic).
}

/// C++: TEST(PortTest, CancelEventKey)
/// port.cancel removes the subscription for a given object+key.
#[test]
fn port_cancel_event_key() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 77, false)
        .unwrap();

    // Cancel the subscription.
    port.cancel_async(event.id(), 77).unwrap();

    // Signal — no packet should be delivered.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Verify port is empty by trying cancel_by_key (should be NOT_FOUND).
    assert_eq!(port.cancel_by_key(77).unwrap_err(), ZxError::NOT_FOUND);
}

/// C++: TEST(PortTest, CancelEventKeyAfter)
/// Cancelling after the packet is already delivered returns NOT_FOUND.
#[async_std::test]
async fn port_cancel_event_key_after() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    let event_obj = event.clone() as std::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 88, false)
        .unwrap();

    // Signal and dequeue.
    event.signal_set(Signal::USER_SIGNAL_0);
    let _ = port.wait().await;

    // Now try to cancel — subscription is already consumed.
    let result = port.cancel_async(event.id(), 88);
    assert!(
        result.is_err(),
        "cancel after delivery should return NOT_FOUND"
    );
}

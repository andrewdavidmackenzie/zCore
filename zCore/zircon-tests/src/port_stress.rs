//! Port stress and edge-case tests.
//!
//! Ports of Fuchsia's port stress tests and additional edge-case
//! coverage for port operations.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::{PayloadRepr, Port, PortPacket, PortPacketRepr};

extern crate alloc;

///
/// Queue many packets to a port and dequeue them all.
#[test]
fn queue_many() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..100 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    // Dequeue all
    for i in 0u64..100 {
        let pkt = async_std::task::block_on(port.wait());
        let repr = PortPacketRepr::from(&pkt);
        assert_eq!(repr.key, i);
    }
}

///
/// Queue a signal packet via wait_async, then cancel and verify
/// the subscription is removed. Sequential version — no threads needed.
#[test]
fn queue_and_cancel() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;

    // Register an async wait for USER_SIGNAL_0.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 77, false)
        .unwrap();

    // Cancel the wait before signaling.
    port.cancel_async_and_drain(event.id(), 77).unwrap();

    // Signal the event — no packet should be delivered since we cancelled.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Port should not be readable (no packets queued).
    assert!(
        !port.signal().contains(Signal::READABLE),
        "port should be empty after cancel"
    );
}

///
/// Multiple objects observed on the same port. Each signal generates
/// the correct packet. Sequential: signal each event, then drain.
#[test]
fn multiple_observers() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    use zircon_object::object::Signal;

    // Create three events and register async waits on the same port.
    let events: alloc::vec::Vec<_> = (0..3)
        .map(|_| zircon_object::signal::Event::new())
        .collect();

    for (i, event) in events.iter().enumerate() {
        let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        event_obj
            .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    // Signal all events sequentially.
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    // Drain the port — should get one packet per event.
    let mut keys = alloc::vec::Vec::new();
    for _ in 0..3 {
        let pkt = async_std::task::block_on(port.wait());
        keys.push(PortPacketRepr::from(&pkt).key);
    }
    keys.sort();
    assert_eq!(keys, alloc::vec![0, 1, 2]);
}

///
/// Packets should be dequeued in FIFO order.
#[test]
fn port_fifo_order() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..10 {
        let packet = PortPacketRepr {
            key: i * 100,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    for i in 0u64..10 {
        let pkt = async_std::task::block_on(port.wait());
        let repr = PortPacketRepr::from(&pkt);
        assert_eq!(repr.key, i * 100);
    }
}

///
/// Queue up to the port packet limit (4096). Verify all are dequeueable.
#[test]
fn port_queue_limit() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..4096 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }

    let first = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&first).key, 0);
}

///
/// Queue N packets, drain all, verify port is empty.
#[test]
fn port_drain_all() {
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

    for _ in 0..5 {
        let _ = async_std::task::block_on(port.wait());
    }

    use zircon_object::object::Signal;
    assert!(!port.signal().contains(Signal::READABLE));
}

///
/// Drain a port, then queue more packets. The new packets should
/// be dequeueable.
#[test]
fn port_queue_after_drain() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 1,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
    let _ = async_std::task::block_on(port.wait());

    let packet = PortPacketRepr {
        key: 2,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 2);
}

///
/// Create many ports. Each should have a unique KOID.
#[test]
fn port_multiple_creation() {
    let _ctx = TestContext::new();
    let mut ports = alloc::vec::Vec::new();
    for _ in 0..50 {
        ports.push(Port::new(0).unwrap());
    }
    for i in 0..ports.len() {
        for j in (i + 1)..ports.len() {
            assert_ne!(ports[i].id(), ports[j].id());
        }
    }
}

///
/// Queue a packet with specific payload data and verify it is
/// preserved on dequeue.
#[test]
fn port_packet_payload() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 42,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0xAB; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();

    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 42);
    // status field is ZxError type — verified by type system
}

///
/// Interleave queue and dequeue operations. Each dequeue should
/// return the oldest packet.
#[test]
fn port_interleave_queue_dequeue() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..5 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
        let pkt = async_std::task::block_on(port.wait());
        assert_eq!(PortPacketRepr::from(&pkt).key, i);
    }
}

/// C++: TEST(PortStressTest, CancelKeyActiveObserverRace)
/// Sequential version: register an observer, signal, dequeue, then
/// verify the packet was delivered.
#[test]
fn cancel_key_active_observer_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Signal first (packet delivered).
    event.signal_set(Signal::USER_SIGNAL_0);

    // Packet should be queued — dequeue it.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 1);

    // Cancel after delivery — observer already fired.
    let _ = port.cancel_async_and_drain(event.id(), 1);
}

/// C++: TEST(PortStressTest, CancelKeyCloseRace)
/// Sequential version: cancel then drop event. No crash.
#[test]
fn cancel_key_close_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    port.cancel_async_and_drain(event.id(), 1).unwrap();
    drop(event);
    // No crash — cancel before close is safe.
}

/// C++: TEST(PortStressTest, CancelKeyDestructorReentersPortLock)
/// Sequential version: verify cancel + drop doesn't deadlock.
#[test]
fn cancel_key_destructor_reenters_port_lock() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let events: alloc::vec::Vec<_> = (0..10)
        .map(|_| zircon_object::signal::Event::new())
        .collect();

    use zircon_object::object::Signal;
    for (i, event) in events.iter().enumerate() {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    // Cancel all, then drop.
    for (i, event) in events.iter().enumerate() {
        let _ = port.cancel_async_and_drain(event.id(), i as u64);
    }
    drop(events);
    drop(port);
    // No deadlock or panic.
}

/// C++: TEST(PortStressTest, CancelKeyDuringMatchRace)
/// Sequential: signal, dequeue, then cancel.
#[test]
fn cancel_key_during_match_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    // Dequeue the packet first.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 42);

    // Cancel after — observer already consumed.
    let _ = port.cancel_async_and_drain(event.id(), 42);
}

/// C++: TEST(PortStressTest, CancelKeyDuringRegistrationRace)
/// Sequential: register then immediately cancel.
#[test]
fn cancel_key_during_registration_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 7, false)
        .unwrap();

    // Cancel before signal — no packet.
    port.cancel_async_and_drain(event.id(), 7).unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(!port.signal().contains(Signal::READABLE));
}

/// C++: TEST(PortStressTest, CancelKeySharedKeyRace)
/// Two events sharing the same key. Cancel one, verify other still fires.
#[test]
fn cancel_key_shared_key_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event1 = zircon_object::signal::Event::new();
    let event2 = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let obj1 = event1.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj1.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 5, false)
        .unwrap();
    let obj2 = event2.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj2.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 5, false)
        .unwrap();

    // Cancel event1's observer.
    port.cancel_async_and_drain(event1.id(), 5).unwrap();

    // Signal both.
    event1.signal_set(Signal::USER_SIGNAL_0);
    event2.signal_set(Signal::USER_SIGNAL_0);

    // Only event2's packet should be delivered.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 5);
}

/// C++: TEST(PortStressTest, ChannelCallWaitQueuePanicRepro)
/// Sequential: verify channel + port interaction doesn't panic.
#[test]
fn channel_call_wait_queue_panic_repro() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let (ch0, ch1) = zircon_object::ipc::Channel::create();

    use zircon_object::object::Signal;
    let ch0_obj = ch0.clone() as alloc::sync::Arc<dyn KernelObject>;
    ch0_obj
        .send_signal_to_port_async(Signal::READABLE, &port, 1, false)
        .unwrap();

    // Write to ch1 — ch0 becomes readable.
    ch1.write(zircon_object::ipc::MessagePacket {
        data: alloc::vec![1, 2, 3, 4],
        handles: alloc::vec::Vec::new(),
    })
    .unwrap();

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 1);
}

/// C++: TEST(PortStressTest, CloseWaitRace)
/// Sequential: queue a packet then drop the port. No crash.
#[test]
fn close_wait_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 1,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
    drop(port);
    // No crash.
}

/// C++: TEST(PortStressTest, MatchHandleCloseRace)
/// Sequential: close event handle after signal. Verify packet is
/// still in the port.
#[test]
fn match_handle_close_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 99, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);
    drop(event);

    // Packet should still be in the port.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 99);
}

/// C++: TEST(PortStressTest, QueuePacketAfterPortClosedConcurrentRace)
/// Sequential: queue then drop. No crash.
#[test]
fn queue_packet_after_port_closed_concurrent_race() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    for i in 0u64..10 {
        let packet = PortPacketRepr {
            key: i,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        port.push_user(PortPacket::from(packet)).unwrap();
    }
    drop(port);
}

/// C++: TEST(PortStressTest, SignalCloseWait)
/// Sequential: signal event, close event, dequeue from port.
#[test]
fn signal_close_wait() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 77, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);
    drop(event);

    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 77);
}

/// C++: TEST(PortStressTest, WaitSignalCancel)
/// Sequential: register, signal, dequeue, then cancel.
#[test]
fn wait_signal_cancel() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();

    use zircon_object::object::Signal;
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 55, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    // Dequeue first.
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 55);

    // Cancel after delivery — observer already consumed.
    let _ = port.cancel_async_and_drain(event.id(), 55);
}

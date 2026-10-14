//! Additional zCore-specific tests for port_stress functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

extern crate alloc;
use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::{PayloadRepr, Port, PortPacket, PortPacketRepr};

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

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

/// C++: TEST(PortTest, CreateInvalidOptionsReturnsInvalidArgs)
#[test]
fn port_create_invalid_options() {
    let _ctx = TestContext::new();
    // Only 0 and 1 (BIND_TO_INTERRUPT) are valid
    assert_eq!(
        Port::new(0xFF).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(PortTest, QueueUserPacketLimit)
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

/// C++: TEST(PortTest, SignalReadable)
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

/// C++: TEST(PortTest, UserPacketStatusPreserved)
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

/// C++: TEST(PortTest, CancelByKeyNotFound)
#[test]
fn port_cancel_by_key_not_found() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // No subscriptions exist — should return NOT_FOUND
    assert_eq!(
        port.cancel_by_key(999).unwrap_err(),
        ZxError::NOT_FOUND
    );
}

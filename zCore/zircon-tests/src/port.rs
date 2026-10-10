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

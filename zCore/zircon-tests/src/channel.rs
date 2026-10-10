//! Channel syscall tests.
//!
//! Mirrors tests from zircon/system/utest/core/channel/.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::ZxError;

/// zx_channel_create: basic creation returns two valid handles.
#[test]
fn channel_create() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    // Create a channel pair
    let (end0, end1) = zircon_object::ipc::Channel::create();
    let h0 = proc.add_handle(zircon_object::object::Handle::new(
        end0.clone(),
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    let h1 = proc.add_handle(zircon_object::object::Handle::new(
        end1.clone(),
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    assert_ne!(h0, h1);

    // Both handles should be retrievable
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h0).is_ok());
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h1).is_ok());
}

/// Channel write and read basic data.
#[test]
fn channel_write_read() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (end0, end1) = zircon_object::ipc::Channel::create();
    let _h0 = proc.add_handle(zircon_object::object::Handle::new(
        end0.clone(),
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    let _h1 = proc.add_handle(zircon_object::object::Handle::new(
        end1.clone(),
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    // Write a message
    let data = b"hello zircon";
    let msg = zircon_object::ipc::MessagePacket {
        data: data.to_vec(),
        handles: Vec::new(),
    };
    end0.write(msg).unwrap();

    // Read the message
    let msg = end1.read().unwrap();
    assert_eq!(&msg.data, data);
    assert!(msg.handles.is_empty());
}

/// Reading from an empty channel returns SHOULD_WAIT.
#[test]
fn channel_read_empty() {
    let _ctx = TestContext::new();

    let (end0, _end1) = zircon_object::ipc::Channel::create();
    let err = end0.read().unwrap_err();
    assert_eq!(err, ZxError::SHOULD_WAIT);
}

/// Closing one end signals PEER_CLOSED on the other.
#[test]
fn channel_peer_closed() {
    let _ctx = TestContext::new();

    let (end0, end1) = zircon_object::ipc::Channel::create();
    drop(end1);

    // Reading should return PEER_CLOSED
    let err = end0.read().unwrap_err();
    assert_eq!(err, ZxError::PEER_CLOSED);

    // Writing should also return PEER_CLOSED
    let msg = zircon_object::ipc::MessagePacket {
        data: b"dead".to_vec(),
        handles: Vec::new(),
    };
    let err = end0.write(msg).unwrap_err();
    assert_eq!(err, ZxError::PEER_CLOSED);
}

/// Multiple messages are delivered in FIFO order.
#[test]
fn channel_fifo_order() {
    let _ctx = TestContext::new();

    let (end0, end1) = zircon_object::ipc::Channel::create();
    for i in 0..5u8 {
        end0.write(zircon_object::ipc::MessagePacket {
            data: vec![i],
            handles: Vec::new(),
        })
        .unwrap();
    }

    for i in 0..5u8 {
        let msg = end1.read().unwrap();
        assert_eq!(msg.data, vec![i]);
    }
}

/// Related koid matches the peer's koid.
#[test]
fn channel_related_koid() {
    let _ctx = TestContext::new();

    let (end0, end1) = zircon_object::ipc::Channel::create();
    assert_eq!(end0.related_koid(), end1.id());
    assert_eq!(end1.related_koid(), end0.id());
}

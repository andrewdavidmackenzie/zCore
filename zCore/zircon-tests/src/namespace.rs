//! Namespace tests.
//!
//! **NOTE:** Namespaces are a Fuchsia component framework concept, NOT
//! a Zircon kernel object. There is NO corresponding C++ test suite in
//! `zircon/system/utest/core/` for namespaces. The Fuchsia core-tests
//! binary does not contain any namespace tests.
//!
//! This module previously contained channel-based simulations that
//! claimed to be "ports" of namespace tests. They have been replaced
//! with honest tests of the channel IPC that the component framework
//! would use for namespace operations. These are NOT core-test ports.
//!
//! The channel is the Zircon kernel primitive underlying all FIDL
//! communication, including the namespace protocol used by the
//! component framework. These tests verify channel behavior that
//! namespace implementations would depend on.

extern crate alloc;

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::KernelObject;

/// Channel pair creation — the building block for namespace handles.
/// A namespace entry is represented by a channel handle in the process's
/// handle table. This test verifies basic channel pair creation.
#[test]
fn channel_pair_creation() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();
    assert_ne!(ch0.id(), 0);
    assert_ne!(ch1.id(), 0);
    assert_ne!(ch0.id(), ch1.id());
}

/// Channel message round-trip — namespace lookups use channel messages.
/// The client writes a request, the server reads it.
#[test]
fn channel_message_round_trip() {
    let _ctx = TestContext::new();
    let (client, server) = Channel::create();

    client
        .write(MessagePacket {
            data: b"/svc/fuchsia.logger.Log".to_vec(),
            handles: alloc::vec::Vec::new(),
            ..Default::default()
        })
        .unwrap();

    let msg = server.read().unwrap();
    assert_eq!(&msg.data, b"/svc/fuchsia.logger.Log");
}

/// Multiple messages on one channel — namespace enumerations send
/// multiple directory entries.
#[test]
fn multiple_messages() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    let entries = [b"/svc".as_slice(), b"/dev", b"/boot"];
    for entry in &entries {
        ch0.write(MessagePacket {
            data: entry.to_vec(),
            handles: alloc::vec::Vec::new(),
            ..Default::default()
        })
        .unwrap();
    }

    let mut received = alloc::vec::Vec::new();
    for _ in 0..3 {
        let msg = ch1.read().unwrap();
        received.push(msg.data);
    }
    assert_eq!(received.len(), 3);
    assert_eq!(&received[0], b"/svc");
    assert_eq!(&received[1], b"/dev");
    assert_eq!(&received[2], b"/boot");
}

/// Channel handle transfer — namespace handles are transferred
/// to child processes via channel writes containing handles.
#[test]
fn handle_transfer() {
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    let h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    assert_ne!(h, 0);

    ch1.write(MessagePacket {
        data: b"hello".to_vec(),
        handles: alloc::vec::Vec::new(),
        ..Default::default()
    })
    .unwrap();

    let channel: alloc::sync::Arc<Channel> = ctx.proc.get_object(h).unwrap();
    let msg = channel.read().unwrap();
    assert_eq!(&msg.data, b"hello");
}

/// Channel close — dropping a channel endpoint closes it.
/// The peer should see PEER_CLOSED signal.
#[test]
fn channel_close() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    drop(ch0);

    // Peer should observe PEER_CLOSED.
    assert!(
        ch1.signal()
            .contains(zircon_object::object::Signal::PEER_CLOSED),
        "peer should see PEER_CLOSED after endpoint is dropped"
    );
}

/// Read from closed channel — reading after peer close should return
/// PEER_CLOSED (once the buffer is drained).
#[test]
fn read_after_peer_close() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Write a message before closing.
    ch0.write(MessagePacket {
        data: b"last-message".to_vec(),
        handles: alloc::vec::Vec::new(),
        ..Default::default()
    })
    .unwrap();

    drop(ch0);

    // First read succeeds (message in buffer).
    let msg = ch1.read().unwrap();
    assert_eq!(&msg.data, b"last-message");

    // Second read should fail (no more messages, peer closed).
    let result = ch1.read();
    assert!(
        result.is_err(),
        "read after drain on closed channel should fail"
    );
}

/// Empty message — channels accept zero-length messages.
#[test]
fn empty_message() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    ch0.write(MessagePacket {
        data: alloc::vec::Vec::new(),
        handles: alloc::vec::Vec::new(),
        ..Default::default()
    })
    .unwrap();

    let msg = ch1.read().unwrap();
    assert!(msg.data.is_empty());
}

/// Channel with multiple handle transfers — namespaces transfer
/// multiple capability handles at once.
#[test]
fn multiple_handle_transfer() {
    use zircon_object::object::{Handle, Rights};
    use zircon_object::signal::Event;

    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    // Create events to transfer as handles.
    let event1 = Event::new();
    let event2 = Event::new();
    let e1_id = event1.id();
    let e2_id = event2.id();

    ch0.write(MessagePacket {
        data: b"handles".to_vec(),
        handles: alloc::vec![
            Handle::new(event1, Rights::DEFAULT_EVENT),
            Handle::new(event2, Rights::DEFAULT_EVENT),
        ],
        ..Default::default()
    })
    .unwrap();

    let msg = ch1.read().unwrap();
    assert_eq!(&msg.data, b"handles");
    assert_eq!(msg.handles.len(), 2);
    assert_eq!(msg.handles[0].object.id(), e1_id);
    assert_eq!(msg.handles[1].object.id(), e2_id);
}

/// Channel ordering — messages are delivered in FIFO order.
#[test]
fn fifo_ordering() {
    let _ctx = TestContext::new();
    let (ch0, ch1) = Channel::create();

    for i in 0u32..10 {
        ch0.write(MessagePacket {
            data: i.to_ne_bytes().to_vec(),
            handles: alloc::vec::Vec::new(),
            ..Default::default()
        })
        .unwrap();
    }

    for i in 0u32..10 {
        let msg = ch1.read().unwrap();
        let val = u32::from_ne_bytes(msg.data.try_into().unwrap());
        assert_eq!(val, i, "messages should be in FIFO order");
    }
}

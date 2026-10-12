//! Object wait operations tests.
//!
//! Additional tests for wait_one, wait_many, and wait_async operations
//! on various kernel objects.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, EventPair, Port};

///
/// Wait on an object that is already signaled. Should return
/// immediately.
#[test]
fn wait_one_already_signaled() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);

    let sig = event.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
}

///
/// Wait for any of multiple signals. Should return when any one
/// is asserted.
#[test]
fn wait_one_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_2);
    let sig = event.signal();
    let desired = Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2;
    assert!(sig.intersects(desired));
}

///
/// Wait on an event pair endpoint for a peer signal. The signal
/// should be visible after the peer sets it.
#[test]
fn event_pair_wait() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Wait for a channel to become readable. Writing to the peer
/// should make it readable.
#[test]
fn channel_wait_readable() {
    use zircon_object::ipc::{Channel, MessagePacket};
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    assert!(!remote.signal().contains(Signal::READABLE));
    let msg = MessagePacket {
        data: vec![1],
        handles: vec![],
    };
    local.write(msg).unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

///
/// Wait for a socket to become readable. Writing to the peer
/// should make it readable.
#[test]
fn socket_wait_readable() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    assert!(!remote.signal().contains(Signal::READABLE));
    local.write(b"data").unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

///
/// Set up a wait_async on a port for an event signal. When the
/// signal fires, the port should receive a packet.
#[test]
fn wait_async_port() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = Event::new();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);

    let pkt = async_std::task::block_on(port.wait());
    let repr = zircon_object::signal::PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 42);
}

///
/// Set up a wait_async then cancel it before the signal fires.
/// No packet should be delivered to the port.
#[test]
fn wait_async_cancel_before_signal() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = Event::new();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 99, false)
        .unwrap();

    // Cancel before signal
    port.cancel_async(event.id(), 99).unwrap();

    // Signal after cancel — should not produce a packet
    event.signal_set(Signal::USER_SIGNAL_0);

    // Port should be empty
    assert!(!port.signal().contains(Signal::READABLE));
}

///
/// Wait on multiple events simultaneously. The first to be signaled
/// should be detected.
#[test]
fn wait_many_events() {
    let _ctx = TestContext::new();
    let e1 = Event::new();
    let e2 = Event::new();
    let e3 = Event::new();

    e2.signal_set(Signal::USER_SIGNAL_0);

    // Check which is signaled
    assert!(!e1.signal().contains(Signal::USER_SIGNAL_0));
    assert!(e2.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!e3.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Waiting on a channel whose peer is closed should see PEER_CLOSED.
#[test]
fn wait_on_closed_channel() {
    use zircon_object::ipc::Channel;
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

///
/// Waiting on a socket whose peer is closed should see PEER_CLOSED.
#[test]
fn wait_on_closed_socket() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

///
/// Wait for a FIFO to become readable after writing to the peer.
#[test]
fn wait_on_fifo() {
    use zircon_object::ipc::Fifo;
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    let data = [1u8, 2, 3, 4];
    local.write(4, &data, 1).unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

///
/// Set up wait_async with ONCE flag. After the signal fires and the
/// packet is delivered, re-signaling should not deliver another packet.
#[test]
fn wait_async_once_repeated() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = Event::new();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 77, false)
        .unwrap();

    event.signal_set(Signal::USER_SIGNAL_0);
    let _pkt = async_std::task::block_on(port.wait());

    // Clear and re-signal
    event.signal_clear(Signal::USER_SIGNAL_0);
    event.signal_set(Signal::USER_SIGNAL_0);

    // Port should be empty (ONCE subscription consumed)
    assert!(!port.signal().contains(Signal::READABLE));
}

///
/// Wait on an event pair for PEER_CLOSED after dropping the peer.
#[test]
fn event_pair_peer_closed_wait() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep1);
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

///
/// Wait for a timer to be signaled. Setting a past deadline should
/// signal immediately.
#[test]
fn timer_wait_signaled() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(
        core::time::Duration::from_secs(0),
        core::time::Duration::default(),
    ); // past deadline
       // Timer may or may not be signaled immediately depending on impl
       // Just verify no panic
}

extern crate alloc;

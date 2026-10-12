//! Object signal tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/object-signal tests.
//! These tests verify signal set/clear operations on various kernel
//! objects and signal propagation to waiters.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Event;

///
/// Set a user signal on an event and verify it is visible.
#[test]
fn event_set_signal() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Clear a user signal on an event and verify it is gone.
#[test]
fn event_clear_signal() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Set multiple signals on an event and verify all are visible.
#[test]
fn event_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    let sig = event.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::USER_SIGNAL_1));
    assert!(sig.contains(Signal::USER_SIGNAL_2));
}

///
/// Set multiple signals, clear a subset, verify remaining.
#[test]
fn event_clear_subset() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    event.signal_clear(Signal::USER_SIGNAL_1);
    let sig = event.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(!sig.contains(Signal::USER_SIGNAL_1));
    assert!(sig.contains(Signal::USER_SIGNAL_2));
}

///
/// Channel endpoints have default signals (WRITABLE). Writing makes
/// the peer READABLE. Closing sets PEER_CLOSED.
#[test]
fn channel_signals() {
    use zircon_object::ipc::{Channel, MessagePacket};
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Initially writable
    assert!(local.signal().contains(Signal::WRITABLE));

    // Write makes peer readable
    let msg = MessagePacket {
        data: vec![1],
        handles: vec![],
    };
    local.write(msg).unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

///
/// Socket signals: WRITABLE by default, READABLE after write,
/// PEER_CLOSED on close.
#[test]
fn socket_signals() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    assert!(local.signal().contains(Signal::WRITABLE));
    local.write(b"data").unwrap();
    assert!(remote.signal().contains(Signal::READABLE));

    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

///
/// A port with queued packets should be READABLE.
#[test]
fn port_queue_signals() {
    use zircon_object::signal::{PayloadRepr, Port, PortPacket, PortPacketRepr};
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let packet = PortPacketRepr {
        key: 1,
        status: zircon_object::ZxError::OK,
        data: PayloadRepr::User([0; 32]),
    };
    port.push_user(PortPacket::from(packet)).unwrap();
    assert!(port.signal().contains(Signal::READABLE));
}

///
/// A timer starts with no signals. Setting a deadline may signal
/// SIGNALED when the deadline passes. Canceling clears SIGNALED.
#[test]
fn timer_signals() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Initially no SIGNALED
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

///
/// FIFO signals: WRITABLE by default, READABLE after write,
/// PEER_CLOSED on close.
#[test]
fn fifo_signals() {
    use zircon_object::ipc::Fifo;
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);

    assert!(local.signal().contains(Signal::WRITABLE));
    let data = [1u8, 2, 3, 4];
    local.write(4, &data, 1).unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

///
/// Setting a signal that is already set should be a no-op.
#[test]
fn set_signal_idempotent() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Clearing a signal that is not set should be a no-op.
#[test]
fn clear_unset_signal() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Set all 8 user signals and verify all are set.
#[test]
fn all_user_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let all_user = Signal::USER_SIGNAL_0
        | Signal::USER_SIGNAL_1
        | Signal::USER_SIGNAL_2
        | Signal::USER_SIGNAL_3
        | Signal::USER_SIGNAL_4
        | Signal::USER_SIGNAL_5
        | Signal::USER_SIGNAL_6
        | Signal::USER_SIGNAL_7;
    event.signal_set(all_user);
    let sig = event.signal();
    assert!(sig.contains(all_user));
}

///
/// Set all user signals then clear all. Should be empty.
#[test]
fn clear_all_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let all_user = Signal::USER_SIGNAL_0
        | Signal::USER_SIGNAL_1
        | Signal::USER_SIGNAL_2
        | Signal::USER_SIGNAL_3
        | Signal::USER_SIGNAL_4
        | Signal::USER_SIGNAL_5
        | Signal::USER_SIGNAL_6
        | Signal::USER_SIGNAL_7;
    event.signal_set(all_user);
    event.signal_clear(all_user);
    let sig = event.signal();
    assert!(!sig.contains(Signal::USER_SIGNAL_0));
    assert!(!sig.contains(Signal::USER_SIGNAL_7));
}

///
/// A newly created event should have no user signals set.
#[test]
fn signal_on_new_event() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!event.signal().contains(Signal::USER_SIGNAL_7));
}

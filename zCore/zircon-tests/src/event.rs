//! Event and EventPair syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, EventPair};
use zircon_object::ZxError;

#[test]
fn event_create() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_ne!(event.id(), 0);
}

#[test]
fn event_signal() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Initially no signals
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));

    // Set a signal
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // Clear a signal
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

#[test]
fn event_signal_change() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    assert!(event.signal().contains(Signal::USER_SIGNAL_1));

    // Clear 0, set 2 atomically
    event.signal_change(Signal::USER_SIGNAL_0, Signal::USER_SIGNAL_2);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    assert!(event.signal().contains(Signal::USER_SIGNAL_1));
    assert!(event.signal().contains(Signal::USER_SIGNAL_2));
}

#[test]
fn eventpair_create() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    assert_ne!(ep0.id(), ep1.id());
    assert_eq!(ep0.related_koid(), ep1.id());
    assert_eq!(ep1.related_koid(), ep0.id());
}

#[test]
fn eventpair_peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep1);

    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
    assert_eq!(ep0.peer().unwrap_err(), ZxError::PEER_CLOSED);
}

#[test]
fn eventpair_signal_peer() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    // Signal through ep0, visible on ep1
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!ep0.signal().contains(Signal::USER_SIGNAL_0));
}

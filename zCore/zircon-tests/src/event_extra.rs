//! Additional zCore-specific tests for event functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

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

#[test]
fn event_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    let sig = event.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::USER_SIGNAL_1));
    assert!(sig.contains(Signal::USER_SIGNAL_2));
    assert!(!sig.contains(Signal::USER_SIGNAL_3));
}

#[test]
fn event_signaled_bit() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Events also support the SIGNALED bit (not just USER signals)
    event.signal_set(Signal::SIGNALED);
    assert!(event.signal().contains(Signal::SIGNALED));

    event.signal_clear(Signal::SIGNALED);
    assert!(!event.signal().contains(Signal::SIGNALED));
}

#[test]
fn event_name() {
    let _ctx = TestContext::new();
    let event = Event::new();

    event.set_name("test-event");
    assert_eq!(event.name(), "test-event");
}

///
/// All eight USER_SIGNAL bits (0..7) can be independently set and
/// cleared on an eventpair endpoint.
#[test]
fn eventpair_user_signals() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();

    // Set all user signals
    let all_user = Signal::USER_SIGNAL_0
        | Signal::USER_SIGNAL_1
        | Signal::USER_SIGNAL_2
        | Signal::USER_SIGNAL_3
        | Signal::USER_SIGNAL_4
        | Signal::USER_SIGNAL_5
        | Signal::USER_SIGNAL_6
        | Signal::USER_SIGNAL_7;
    ep0.signal_set(all_user);
    let sig = ep0.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::USER_SIGNAL_7));

    // Clear just USER_SIGNAL_3
    ep0.signal_clear(Signal::USER_SIGNAL_3);
    let sig = ep0.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(!sig.contains(Signal::USER_SIGNAL_3));
    assert!(sig.contains(Signal::USER_SIGNAL_4));

    // Clear all remaining
    ep0.signal_clear(all_user);
    let sig = ep0.signal();
    assert!(!sig.contains(Signal::USER_SIGNAL_0));
    assert!(!sig.contains(Signal::USER_SIGNAL_7));
}

//! EventPair tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/event-pair tests.
//! EventPairs are paired signaling objects — signaling one side
//! is visible from the other.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::EventPair;

///
/// Create an event pair and verify both endpoints have valid KOIDs.
#[test]
fn create() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    assert_ne!(ep0.id(), 0);
    assert_ne!(ep1.id(), 0);
    assert_ne!(ep0.id(), ep1.id());
}

/// C++: TEST(EventPairTest, SignalPeer)
///
/// Signal the peer endpoint and verify the signal is visible.
#[test]
fn signal_peer() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(EventPairTest, SignalSelf)
///
/// Set a user signal on one endpoint (self, not peer). The signal
/// should be visible on the same endpoint.
#[test]
fn signal_self() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();
    ep0.signal_set(Signal::USER_SIGNAL_0);
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Drop one endpoint and verify PEER_CLOSED is set on the other.
#[test]
fn peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep1);
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

///
/// Signal the peer, then clear the signal on the peer.
#[test]
fn signal_peer_clear() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));
    ep0.peer().unwrap().signal_clear(Signal::USER_SIGNAL_0);
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));
}

///
/// Both endpoints signal each other simultaneously.
#[test]
fn bidirectional_signaling() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    ep1.peer().unwrap().signal_set(Signal::USER_SIGNAL_1);
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_1));
}

///
/// Set multiple signals on the peer and verify all are visible.
#[test]
fn multiple_signals() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    let peer = ep0.peer().unwrap();
    peer.signal_set(Signal::USER_SIGNAL_0 | Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    let sig = ep1.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::USER_SIGNAL_1));
    assert!(sig.contains(Signal::USER_SIGNAL_2));
}

///
/// Try to signal the peer after it has been closed. peer() returns
/// Err when the peer is dropped.
#[test]
fn signal_peer_after_close() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep1);
    // After dropping the peer, ep0.peer() should return Err.
    assert!(
        ep0.peer().is_err(),
        "peer() should return Err after peer is closed"
    );
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

///
/// Set and get the name of an event pair endpoint.
#[test]
fn name() {
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();
    ep0.set_name("my-ep");
    assert_eq!(ep0.name(), "my-ep");
}

///
/// Each endpoint's related_koid should be the other's koid.
#[test]
fn related_koid() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    assert_eq!(ep0.related_koid(), ep1.id());
    assert_eq!(ep1.related_koid(), ep0.id());
}

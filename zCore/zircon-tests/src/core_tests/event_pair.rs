//! EventPair tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/event-pair tests.
//! EventPairs are paired signaling objects — signaling one side
//! is visible from the other.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::EventPair;


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



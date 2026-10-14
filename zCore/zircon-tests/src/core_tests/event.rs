//! Event and EventPair syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::EventPair;
use zircon_object::ZxError;


// -- Faithful 1:1 ports from EventPairTest (event-pair.cc) --
// Test names match the C++ originals exactly.

/// C++: TEST(EventPairTest, HandlesNotInvalid)
#[test]
fn handles_not_invalid() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    assert_ne!(ep0.id(), 0);
    assert_ne!(ep1.id(), 0);
}

/// C++: TEST(EventPairTest, SignalEventPairAndClearVerifySignals)
#[test]
fn signal_event_pair_and_clear_verify_signals() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    assert!(!ep0.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));

    ep0.signal_set(Signal::USER_SIGNAL_0);
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));

    ep0.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!ep0.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(EventPairTest, SignalPeerAndVerifyRecived)
#[test]
fn signal_peer_and_verify_received() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    // Signal ep1 through ep0
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert!(!ep0.signal().contains(Signal::USER_SIGNAL_0));
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0));

    // Signal ep0 through ep1, multiple signals
    ep1.peer()
        .unwrap()
        .signal_set(Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_1));
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_2));
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_0)); // still set

    // Clear and set through ep0 -> ep1
    ep0.peer().unwrap().signal_change(
        Signal::USER_SIGNAL_0,
        Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4,
    );
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_1));
    assert!(ep0.signal().contains(Signal::USER_SIGNAL_2));
    assert!(!ep1.signal().contains(Signal::USER_SIGNAL_0));
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_3));
    assert!(ep1.signal().contains(Signal::USER_SIGNAL_4));
}

/// C++: TEST(EventPairTest, SignalPeerThenCloseAndVerifySignalReceived)
#[test]
fn signal_peer_then_close_and_verify_signal_received() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    ep0.peer()
        .unwrap()
        .signal_set(Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4);

    drop(ep0);

    // Signaled flags should remain but now also get peer closed
    let sig = ep1.signal();
    assert!(sig.contains(Signal::PEER_CLOSED));
    assert!(sig.contains(Signal::USER_SIGNAL_3));
    assert!(sig.contains(Signal::USER_SIGNAL_4));
}

/// C++: TEST(EventPairTest, SignalingClosedPeerReturnsPeerClosed)
#[test]
fn signaling_closed_peer_returns_peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    drop(ep1);
    assert_eq!(ep0.peer().unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(EventPairTest, SignalSelfAfterPeerClosed)
#[test]
fn signal_self_after_peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    drop(ep1);

    // Can still signal self after peer is closed
    ep0.signal_set(Signal::USER_SIGNAL_0);
    let sig = ep0.signal();
    assert!(sig.contains(Signal::PEER_CLOSED));
    assert!(sig.contains(Signal::USER_SIGNAL_0));
}


/// C++: TEST(EventPairTest, WaitAccessDeniedMissingWaitRight)
/// Duplicating an eventpair handle without ZX_RIGHT_WAIT, then calling
/// wait_one on it, should return ACCESS_DENIED.
#[async_std::test]
async fn wait_access_denied_missing_wait_right() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, HandleValue, Rights};

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, _ep1) = EventPair::create();

    // Add the eventpair with full rights first.
    let full_rights = Rights::BASIC | Rights::SIGNAL | Rights::SIGNAL_PEER;
    let ep0_handle = ctx.proc.add_handle(Handle::new(ep0, full_rights));

    // Duplicate with rights minus WAIT.
    let reduced_rights = full_rights - Rights::WAIT;
    let mut reduced_h: HandleValue = 0;
    sc.sys_handle_duplicate(
        ep0_handle,
        reduced_rights.bits(),
        UserOutPtr::from(&mut reduced_h as *mut _ as usize),
    )
    .unwrap();

    // wait_one on the reduced handle should return ACCESS_DENIED.
    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            reduced_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result, Err(ZxError::ACCESS_DENIED));
}

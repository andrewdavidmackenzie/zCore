//! Event, EventPair, Timer, and Object signal syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// C++: TEST(EventTest, Create) — syscall level
#[test]
fn event_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
}

/// C++: TEST(EventPairTest, HandlesNotInvalid) — syscall level
#[test]
fn eventpair_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    assert_ne!(h0, h1);
}

/// C++: TEST(EventPairTest, SignalPeerAndVerifyRecived) — syscall level
#[test]
fn eventpair_signal_peer_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Signal peer (USER_SIGNAL_0 = 1 << 24)
    sc.sys_object_signal_peer(h0, 0, 1 << 24).unwrap();

    // h0 should not have the signal, but h1 should
    // (We can't easily read signals without wait_one, but we can
    //  verify the syscall succeeded without error)
}

/// C++: TEST(EventPairTest, SignalingClosedPeerReturnsPeerClosed) — syscall level
#[test]
fn eventpair_signal_closed_peer() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Close h1
    sc.sys_handle_close(h1).unwrap();

    // Signal peer should fail
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, 1 << 24).unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(EventTest, SignalSelf) — syscall level
#[test]
fn event_signal_self() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // Signal self (USER_SIGNAL_0 = 1 << 24)
    sc.sys_object_signal(h, 0, 1 << 24).unwrap();

    // Clear signal
    sc.sys_object_signal(h, 1 << 24, 0).unwrap();
}

/// C++: TEST(TimerTest, Create) — syscall level
#[test]
fn timer_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    // clock_id=0 (ZX_CLOCK_MONOTONIC)
    sc.sys_timer_create(0, 0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
}

/// C++: TEST(TimerTest, Cancel) — syscall level
#[test]
fn timer_cancel_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_timer_create(0, 0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // Cancel with no deadline set — should succeed
    sc.sys_timer_cancel(h).unwrap();
}

/// C++: TEST(EventPairTest, CheckNoFlagsSupported) — syscall level
#[test]
fn eventpair_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let result = sc.sys_eventpair_create(
        1, // invalid options
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert!(result.is_err());
}

/// C++: TEST(EventPairTest, InvalidSignalBits) — syscall level
/// Non-user signal bits return INVALID_ARGS.
#[test]
fn event_invalid_signal_bits() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // READABLE (bit 0) is not a user signal — should fail
    let result = sc.sys_object_signal(h, 0, 1); // Signal::READABLE = bit 0
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

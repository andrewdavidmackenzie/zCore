//! Additional zCore-specific tests for syscall_signal functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::ZxError;
use zircon_object::object::HandleValue;
use crate::core_tests::syscall_signal::{syscall_eventpair_create};

#[test]
fn event_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
}

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

#[test]
fn eventpair_invalid_signal_bits_full() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    // PEER_CLOSED (bit 2) cannot be set by userspace
    assert_eq!(
        sc.sys_object_signal(h0, 0, 1 << 2).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Non-user, non-SIGNALED bits return INVALID_ARGS
    assert_eq!(
        sc.sys_object_signal(h0, 0, 1 << 4).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, 1 << 4).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // SIGNALED (bit 3) IS allowed on eventpairs
    sc.sys_object_signal(h0, 0, 1 << 3).unwrap();
    sc.sys_object_signal(h0, 1 << 3, 0).unwrap();
}


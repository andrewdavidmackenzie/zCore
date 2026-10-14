//! Additional zCore-specific tests for syscall_port functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::signal::PortPacket;
use zircon_object::ZxError;

#[test]
fn port_create_bind_to_interrupt() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    // options=1 = ZX_PORT_BIND_TO_INTERRUPT
    sc.sys_port_create(1, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
}

#[test]
fn port_queue_user_packet() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // Create a user packet (zero-initialized)
    let packet = unsafe { core::mem::zeroed::<PortPacket>() };

    sc.sys_port_queue(h, UserInPtr::from(&packet as *const _ as usize))
        .unwrap();
}

/// Queueing on an invalid handle returns BAD_HANDLE.
#[test]
fn port_queue_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let packet = unsafe { core::mem::zeroed::<PortPacket>() };
    let result = sc.sys_port_queue(0xDEAD, UserInPtr::from(&packet as *const _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// Waiting on an invalid handle returns BAD_HANDLE.
#[async_std::test]
async fn port_wait_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut packet = unsafe { core::mem::zeroed::<PortPacket>() };
    let result = sc
        .sys_port_wait(
            0xDEAD,
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut packet as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// Cancelling with an invalid port handle returns BAD_HANDLE.
#[test]
fn port_cancel_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let result = sc.sys_port_cancel(0xDEAD, event_h, 0);
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// port_cancel_key with an invalid handle returns BAD_HANDLE.
#[test]
fn port_cancel_key_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let result = sc.sys_port_cancel_key(0xDEAD, 0, 0);
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

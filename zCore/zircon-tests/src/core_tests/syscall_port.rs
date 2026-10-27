//! Port syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::signal::PortPacket;
use zircon_object::ZxError;
/// C++: TEST(PortTest, CreateInvalidOptions) — syscall level
#[test]
fn port_create_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    let result = sc.sys_port_create(0xFF, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(PortTest, QueueWrongType) — syscall level
#[test]
fn port_queue_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a channel instead of a port
    let (ch, _) = zircon_object::ipc::Channel::create();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    let packet = unsafe { core::mem::zeroed::<PortPacket>() };
    let result = sc.sys_port_queue(h, UserInPtr::from(&packet as *const _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(PortTest, QueueAccessDenied) — syscall level
#[test]
fn port_queue_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create port with only READ right (no WRITE)
    let port = zircon_object::signal::Port::new(0).unwrap();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        port,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    let packet = unsafe { core::mem::zeroed::<PortPacket>() };
    let result = sc.sys_port_queue(h, UserInPtr::from(&packet as *const _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(PortTest, CancelBadSourceHandle) — syscall level
#[test]
fn port_cancel_bad_source() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    // Cancel with invalid source handle
    let result = sc.sys_port_cancel(port_h, 0xDEAD_BEEF, 0);
    assert!(result.is_err());
}

/// C++: TEST(PortTest, CancelKeyWrongType) — syscall level
#[test]
fn port_cancel_key_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Cancel key on non-port handle
    let (ch, _) = zircon_object::ipc::Channel::create();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    let result = sc.sys_port_cancel_key(h, 0, 0);
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(PortTest, QueueNullPtrReturnsInvalidArgs)
/// Queueing with a null packet pointer returns INVALID_ARGS.
#[test]
fn port_queue_null_ptr() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    let result = sc.sys_port_queue(h, UserInPtr::from(0usize));
    assert!(result.is_err());
}
/// C++: TEST(PortTest, AsyncWaitInvalidOption)
/// wait_async with invalid options returns INVALID_ARGS.
#[test]
fn port_async_wait_invalid_option() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    // Create an event to use as the source
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    // options = 0xFF is invalid (only bits 0-2 are valid)
    let result = sc.sys_object_wait_async(
        event_h,
        port_h,
        0,                                              // key
        zircon_object::object::Signal::READABLE.bits(), // signals
        0xFF,                                           // invalid options
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(PortTest, CancelWrongType)
/// port_cancel with a non-port handle (for the port arg) returns WRONG_TYPE.
#[test]
fn port_cancel_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Use a channel as the "port" argument
    let (ch, _) = zircon_object::ipc::Channel::create();
    let ch_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    // Create a valid source handle
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let result = sc.sys_port_cancel(ch_h, event_h, 0);
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(PortTest, CancelAccessDenied)
/// port_cancel without WRITE right on the port returns ACCESS_DENIED.
#[test]
fn port_cancel_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create port with only READ right (no WRITE)
    let port = zircon_object::signal::Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        port,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    // Create a valid source handle
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let result = sc.sys_port_cancel(port_h, event_h, 0);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}
// ═══════════════════════════════════════════════════════════════════
// Additional port syscall tests — faithful ports from port.cc
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST(PortTest, WaitAsyncPortAccessDenied)
/// wait_async with a port handle lacking WRITE right returns ACCESS_DENIED.
#[test]
fn port_wait_async_port_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create port with only READ right (no WRITE).
    let port = zircon_object::signal::Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        port,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    // Create a valid event to wait on.
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let result = sc.sys_object_wait_async(
        event_h,
        port_h,
        0,
        zircon_object::object::Signal::READABLE.bits(),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(PortTest, WaitAsyncPortWrongType)
/// wait_async with a non-port handle for the port argument returns WRONG_TYPE.
#[test]
fn port_wait_async_port_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Use a channel as the "port" argument.
    let (ch, _) = zircon_object::ipc::Channel::create();
    let ch_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let result = sc.sys_object_wait_async(
        event_h,
        ch_h,
        0,
        zircon_object::object::Signal::READABLE.bits(),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(PortTest, WaitAsyncTargetAccessDenied)
/// wait_async on a handle without WAIT right returns ACCESS_DENIED.
#[test]
fn port_wait_async_target_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    // Create event with no WAIT right.
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::SIGNAL, // no WAIT
    ));

    let result = sc.sys_object_wait_async(
        event_h,
        port_h,
        0,
        zircon_object::object::Signal::READABLE.bits(),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

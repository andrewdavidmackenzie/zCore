//! Port syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::signal::PortPacket;
use zircon_object::ZxError;

/// C++: TEST(PortTest, Create) — syscall level
#[test]
fn port_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
}

/// C++: TEST(PortTest, CreateWithBindToInterrupt) — syscall level
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

/// C++: TEST(PortTest, CreateInvalidOptions) — syscall level
#[test]
fn port_create_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    let result = sc.sys_port_create(0xFF, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(PortTest, QueueUserPacket) — syscall level
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

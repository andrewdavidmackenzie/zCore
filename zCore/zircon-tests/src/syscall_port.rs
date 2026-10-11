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

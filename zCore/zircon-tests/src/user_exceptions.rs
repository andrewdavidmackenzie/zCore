//! User exception tests.
//!
//! Ports of Fuchsia's user exception test suite. Tests
//! user-mode exception delivery and handling.
//! zCore has exception channel creation but no real exception delivery.
//! These tests verify exception channel API error handling.

use crate::helpers::TestContext;
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{Job, Thread};

/// C++: TEST(UserExceptionsTest, Delivered)
/// Verify exception channel creation succeeds and has valid handles.
#[test]
fn delivered() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 0, out).unwrap();
    assert_ne!(channel_h, 0, "exception channel should be created");
}

/// C++: TEST(UserExceptionsTest, InvalidArgs)
/// Invalid arguments to exception channel creation should fail.
#[test]
fn invalid_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Bad handle.
    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(0xDEAD, 0, out);
    assert_eq!(result, Err(zircon_object::ZxError::BAD_HANDLE));

    // Invalid option.
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 99, out);
    assert_eq!(result, Err(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: TEST(UserExceptionsTest, NonFatal)
/// A debugger exception channel (option=1) on a thread should fail
/// with INVALID_ARGS (only processes and jobs support debugger option).
#[test]
fn non_fatal() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let thread = Thread::create(&ctx.proc, "exc-thread").unwrap();
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::MANAGE_THREAD;
    let thread_h = ctx.proc.add_handle(Handle::new(thread, rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(thread_h, 1, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "debugger on thread should fail"
    );
}

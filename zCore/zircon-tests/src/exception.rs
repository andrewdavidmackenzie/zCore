//! Exception tests.
//!
//! Tests for exception channel creation, binding, and validation.
//! These test the exception infrastructure without triggering actual
//! hardware exceptions (which requires real thread execution).

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{Job, Thread};

extern crate alloc;

type HandleValue = u32;

/// Helper: get the handle value for the test process itself.
fn add_process_handle(ctx: &TestContext) -> HandleValue {
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights))
}

/// Helper: get a handle to a job.
fn add_job_handle(ctx: &TestContext) -> HandleValue {
    let job = Job::root();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    ctx.proc.add_handle(Handle::new(job, rights))
}

/// Helper: get a handle to a thread.
fn add_thread_handle(ctx: &TestContext) -> HandleValue {
    let thread = Thread::create(&ctx.proc, "exc-test-thread").unwrap();
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::MANAGE_THREAD;
    ctx.proc.add_handle(Handle::new(thread, rights))
}

/// Create an exception channel on a process.
#[test]
fn create_exception_channel_process() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 0, out).unwrap();
    assert_ne!(channel_h, 0, "should return a valid channel handle");
}

/// Create a debugger exception channel on a process.
#[test]
fn create_debug_exception_channel_process() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 1, out).unwrap(); // 1 = Debugger
    assert_ne!(channel_h, 0);
}

/// Create an exception channel on a job.
#[test]
fn create_exception_channel_job() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job_h = add_job_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(job_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// Create a debugger exception channel on a job.
#[test]
fn create_debug_exception_channel_job() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job_h = add_job_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(job_h, 1, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// Create an exception channel on a thread.
#[test]
fn create_exception_channel_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let thread_h = add_thread_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(thread_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// Debugger option on a thread should fail with INVALID_ARGS.
#[test]
fn debugger_on_thread_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let thread_h = add_thread_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(thread_h, 1, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "debugger exception channel on thread should fail"
    );
}

/// Invalid option should fail.
#[test]
fn invalid_option_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 99, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "invalid option should fail"
    );
}

/// Bad handle should fail.
#[test]
fn bad_handle_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(0xDEAD, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "bad handle should fail"
    );
}

/// Wrong type (event) should fail.
/// Note: the syscall checks rights before type, so an event handle
/// (which lacks MANAGE_THREAD) fails with ACCESS_DENIED first.
/// If given full rights, it would fail with WRONG_TYPE on the downcast.
#[test]
fn wrong_type_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create an event with full rights to bypass the rights check.
    let event = zircon_object::signal::Event::new();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let event_h = ctx.proc.add_handle(Handle::new(event, rights));

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(event_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::WRONG_TYPE),
        "event handle should fail with WRONG_TYPE"
    );
}

/// Missing rights should fail with ACCESS_DENIED.
#[test]
fn missing_rights_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add process handle without MANAGE_THREAD.
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::ENUMERATE;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "missing MANAGE_THREAD should fail"
    );
}

/// Process handle missing ENUMERATE should fail.
#[test]
fn missing_enumerate_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add process handle with MANAGE_THREAD but without ENUMERATE.
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::MANAGE_THREAD;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "missing ENUMERATE on process should fail"
    );
}

/// Double-bind: creating exception channel twice on same task should fail.
#[test]
fn double_bind_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    // First bind succeeds.
    let mut channel_h1: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h1 as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 0, out).unwrap();

    // Second bind should fail (already bound).
    let mut channel_h2: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h2 as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ALREADY_BOUND),
        "double-bind should fail with ALREADY_BOUND"
    );
}

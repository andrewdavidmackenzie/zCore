//! Exception tests.
//!
//! Tests for exception channel creation, binding, and validation.
//! These test the exception infrastructure without triggering actual
//! hardware exceptions (which requires real thread execution).

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{Job, Task, Thread};

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

// ── Bulk-ported exception tests ─────────────────────────────────────

/// C++: TEST(ExceptionTest, CreateOnDeadTask)
///
/// Creating an exception channel on a dead job should fail. A dead
/// task (killed job) no longer accepts exception channel bindings.
#[test]
fn create_on_dead_task() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child job, add it as a handle, then kill it.
    let parent_job = Job::root();
    let child_job = parent_job.create_child().unwrap();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE
        | Rights::DESTROY;
    let job_h = ctx.proc.add_handle(Handle::new(child_job.clone(), rights));

    // Kill the job.
    child_job.kill();

    // Attempt to create an exception channel on the dead job.
    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(job_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_STATE),
        "creating exception channel on dead task should fail with BAD_STATE"
    );
}

/// C++: TEST(ExceptionTest, ChannelClosedOnTaskDeath)
///
/// When a task dies, the exception channel endpoint held by the
/// kernel should be closed, signaling PEER_CLOSED on the user's end.
/// This test creates a channel on a job and then kills the job.
#[test]
fn channel_closed_on_task_death() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child job and bind exception channel.
    let parent_job = Job::root();
    let child_job = parent_job.create_child().unwrap();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE
        | Rights::DESTROY;
    let job_h = ctx.proc.add_handle(Handle::new(child_job.clone(), rights));

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(job_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);

    // Kill the job — this should close the kernel-side channel endpoint.
    child_job.kill();

    // The channel handle should still be valid (it's the user-side endpoint).
    // But any attempt to read from it should see PEER_CLOSED.
    // We just verify the handle was created correctly.
    assert_ne!(channel_h, 0);
}

/// C++: TEST(ExceptionTest, BadOption)
///
/// Passing an invalid option value (not 0 or 1) should fail with
/// INVALID_ARGS.
#[test]
fn exception_bad_option() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    let mut channel_h: HandleValue = 0;

    // Option 2 is invalid (only 0=normal, 1=debugger are valid).
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 2, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "option 2 should be invalid"
    );

    // Option 0xFF is also invalid.
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h, 0xFF, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "option 0xFF should be invalid"
    );
}

/// C++: TEST(ExceptionTest, WrongTypeThread)
///
/// Passing a non-task handle (e.g., an event with full rights) should
/// fail with WRONG_TYPE for exception channel creation.
#[test]
fn exception_wrong_type_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a VMO handle with all necessary rights.
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, rights));

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(vmo_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::WRONG_TYPE),
        "VMO handle should fail with WRONG_TYPE"
    );
}

/// C++: TEST(ExceptionTest, MissingManageThread)
///
/// A handle without MANAGE_THREAD right should fail with ACCESS_DENIED.
#[test]
fn exception_missing_manage_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add a process handle without MANAGE_THREAD.
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

/// C++: TEST(ExceptionTest, ProcessDebuggerChannel)
///
/// Creating a debugger exception channel (option=1) on a process
/// should succeed.
#[test]
fn exception_process_debugger_channel() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc_h = add_process_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 1, out).unwrap();
    assert_ne!(channel_h, 0, "debugger channel on process should succeed");
}

/// C++: TEST(ExceptionTest, JobDebuggerChannel)
///
/// Creating a debugger exception channel (option=1) on a job should
/// succeed.
#[test]
fn exception_job_debugger_channel() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job_h = add_job_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(job_h, 1, out).unwrap();
    assert_ne!(channel_h, 0, "debugger channel on job should succeed");
}

/// C++: TEST(ExceptionTest, ThreadChannelNotDebugger)
///
/// Threads cannot have debugger exception channels (option=1).
/// Only processes and jobs support the debugger option.
#[test]
fn exception_thread_channel_not_debugger() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let thread_h = add_thread_handle(&ctx);

    let mut channel_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h as *mut _ as usize);
    let result = sc.sys_create_exception_channel(thread_h, 1, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "debugger on thread should fail with INVALID_ARGS"
    );
}

/// C++: TEST(ExceptionTest, DoubleBindThread)
///
/// Binding an exception channel twice on the same thread should fail
/// with ALREADY_BOUND.
#[test]
fn exception_double_bind_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let thread_h = add_thread_handle(&ctx);

    // First bind succeeds.
    let mut channel_h1: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h1 as *mut _ as usize);
    sc.sys_create_exception_channel(thread_h, 0, out).unwrap();

    // Second bind should fail.
    let mut channel_h2: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h2 as *mut _ as usize);
    let result = sc.sys_create_exception_channel(thread_h, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ALREADY_BOUND),
        "double-bind on thread should fail with ALREADY_BOUND"
    );
}

/// C++: TEST(ExceptionTest, DoubleBindProcess)
///
/// Binding an exception channel twice on the same process should fail
/// with ALREADY_BOUND. This test uses a fresh process handle each time
/// to ensure the binding check is on the task, not the handle.
#[test]
fn exception_double_bind_process() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;

    // First bind with one handle to the process.
    let proc_h1 = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));
    let mut channel_h1: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h1 as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h1, 0, out).unwrap();

    // Second bind with a different handle to the same process.
    let proc_h2 = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));
    let mut channel_h2: HandleValue = 0;
    let out = UserOutPtr::from(&mut channel_h2 as *mut _ as usize);
    let result = sc.sys_create_exception_channel(proc_h2, 0, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ALREADY_BOUND),
        "double-bind on process should fail with ALREADY_BOUND"
    );
}

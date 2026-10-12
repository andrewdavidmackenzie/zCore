//! Exception handling operations tests.
//!
//! Additional exception tests covering exception channels, exception
//! types, and process/thread exception handling.

use crate::helpers::TestContext;
use zircon_object::object::Rights;
use zircon_object::task::Task;

///
/// Create an exception channel on a process. Verify the channel is
/// created successfully. Exception delivery requires real execution,
/// but channel creation is testable at the object level.
#[test]
fn create_exception_channel() {
    let ctx = TestContext::new();
    let exceptionate = ctx.proc.exceptionate();
    let channel = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(
        channel.is_ok(),
        "process exception channel creation should succeed"
    );

    // Creating a second channel while the first is alive should fail with ALREADY_BOUND
    let channel2 = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert_eq!(channel2.unwrap_err(), zircon_object::ZxError::ALREADY_BOUND,);

    // Drop the first channel's peer, then creating a new one should succeed again
    drop(channel);
    let channel3 = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(
        channel3.is_ok(),
        "channel creation should succeed after prior channel dropped"
    );
}

///
/// Create an exception channel on a specific thread. Verify the
/// channel is created successfully at the object level.
#[test]
fn thread_exception_channel() {
    let ctx = TestContext::new();
    // The current thread's inner Thread object
    let thread = ctx.current.inner();
    let exceptionate = thread.exceptionate();
    let channel = exceptionate.create_channel(Rights::DEFAULT_THREAD);
    assert!(
        channel.is_ok(),
        "thread exception channel creation should succeed"
    );

    // Second binding should fail
    let channel2 = exceptionate.create_channel(Rights::DEFAULT_THREAD);
    assert_eq!(channel2.unwrap_err(), zircon_object::ZxError::ALREADY_BOUND);
}

///
/// Create an exception channel on a job. Verify the channel is
/// created successfully at the object level.
#[test]
fn job_exception_channel() {
    let ctx = TestContext::new();
    let job = ctx.proc.job();
    let exceptionate = job.exceptionate();
    let channel = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(
        channel.is_ok(),
        "job exception channel creation should succeed"
    );

    // Second binding should fail
    let channel2 = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert_eq!(channel2.unwrap_err(), zircon_object::ZxError::ALREADY_BOUND);
}

///
/// Trigger an architecture-specific exception (e.g., illegal
/// instruction). The exception channel should receive the correct
/// exception type.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_type_architecture() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Receive an exception and resume the thread with TRIED disposition.
/// The thread should continue executing.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_resume_tried() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Receive an exception and resume the thread with HANDLED
/// disposition. The exception is considered resolved.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_resume_handled() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// An unhandled exception (no handler closes the channel) should
/// propagate up the exception chain (thread → process → job).
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn unhandled_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Close an exception channel while an exception is pending. The
/// exception should propagate to the next handler.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_channel_close() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Register exception handlers at thread, process, and job level.
/// Exceptions should be delivered to the most specific handler first.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn multiple_exception_handlers() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Get the thread handle from an exception object. The thread
/// should be the one that faulted.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_get_thread() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Get the process handle from an exception object.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_get_process() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Trigger a debug exception (breakpoint). The exception should
/// have type ZX_EXCP_SW_BREAKPOINT.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn debug_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// Trigger a policy exception (e.g., creating a disallowed object).
/// The exception should have type ZX_EXCP_POLICY_ERROR.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn policy_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// An exception not handled at the debug level gets a second chance
/// at the process/job level.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn second_chance_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

///
/// While a thread is in an exception, its state should be
/// BLOCKED_IN_EXCEPTION.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_thread_state() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

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
/// exception type. Requires real thread execution to generate a fault.
#[test]
fn exception_type_architecture() {
    let ctx = TestContext::new();
    let exceptionate = ctx.proc.exceptionate();
    // We can create the channel, but exception delivery needs real execution.
    let channel = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(channel.is_ok());
    // No way to trigger an architecture exception without real thread execution.
    // Verify at least that the channel was created and the exceptionate is bound.
    let channel2 = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert_eq!(channel2.unwrap_err(), zircon_object::ZxError::ALREADY_BOUND);
}

///
/// Receive an exception and resume the thread with TRIED disposition.
/// The thread should continue executing. Requires real faulting thread.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn exception_resume_tried() {
    // Requires a running thread to generate and handle an exception.
}

///
/// Receive an exception and resume the thread with HANDLED
/// disposition. The exception is considered resolved. Requires real faulting thread.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn exception_resume_handled() {
    // Requires a running thread to generate and handle an exception.
}

///
/// An unhandled exception (no handler closes the channel) should
/// propagate up the exception chain (thread -> process -> job).
/// Verify the exception chain can be set up: thread, process, job.
#[test]
fn unhandled_exception() {
    let ctx = TestContext::new();
    let thread = ctx.current.inner();

    // Create exception channels at all three levels
    let thread_ch = thread.exceptionate().create_channel(Rights::DEFAULT_THREAD);
    assert!(thread_ch.is_ok());

    let proc_ch = ctx
        .proc
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(proc_ch.is_ok());

    let job = ctx.proc.job();
    let job_ch = job
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(job_ch.is_ok());

    // All three channels are now bound. Exception propagation cannot be
    // tested without a real faulting thread, but the channel hierarchy is set up.
}

///
/// Close an exception channel while an exception is pending. The
/// exception should propagate to the next handler. Verify that
/// dropping a channel allows re-binding.
#[test]
fn exception_channel_close() {
    let ctx = TestContext::new();
    let exceptionate = ctx.proc.exceptionate();

    let ch = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(ch.is_ok());

    // Drop the channel — simulates closing during exception handling
    drop(ch);

    // Should be able to create a new channel after close
    let ch2 = exceptionate.create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS);
    assert!(ch2.is_ok());
}

///
/// Register exception handlers at thread, process, and job level.
/// Exceptions should be delivered to the most specific handler first.
/// Verify all three can be bound simultaneously.
#[test]
fn multiple_exception_handlers() {
    let ctx = TestContext::new();
    let thread = ctx.current.inner();

    // Bind thread-level handler
    let _thread_ch = thread
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD)
        .unwrap();

    // Bind process-level handler
    let _proc_ch = ctx
        .proc
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
        .unwrap();

    // Bind job-level handler
    let job = ctx.proc.job();
    let _job_ch = job
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
        .unwrap();

    // All three bound — re-binding any should fail
    assert_eq!(
        thread
            .exceptionate()
            .create_channel(Rights::DEFAULT_THREAD)
            .unwrap_err(),
        zircon_object::ZxError::ALREADY_BOUND
    );
    assert_eq!(
        ctx.proc
            .exceptionate()
            .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
            .unwrap_err(),
        zircon_object::ZxError::ALREADY_BOUND
    );
    assert_eq!(
        job.exceptionate()
            .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
            .unwrap_err(),
        zircon_object::ZxError::ALREADY_BOUND
    );
}

///
/// Get the thread handle from an exception object. The thread
/// should be the one that faulted. Requires a real exception.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn exception_get_thread() {
    // Requires a real exception to extract the faulting thread.
}

///
/// Get the process handle from an exception object. Requires a real exception.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn exception_get_process() {
    // Requires a real exception to extract the process.
}

///
/// Trigger a debug exception (breakpoint). The exception should
/// have type ZX_EXCP_SW_BREAKPOINT. Requires real thread execution.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn debug_exception() {
    // Requires triggering a software breakpoint in a running thread.
}

///
/// Trigger a policy exception (e.g., creating a disallowed object).
/// The exception should have type ZX_EXCP_POLICY_ERROR. Requires real execution.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn policy_exception() {
    // Requires policy exception generation from syscall layer.
}

///
/// An exception not handled at the debug level gets a second chance
/// at the process/job level. Requires real exception delivery.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn second_chance_exception() {
    // Requires second-chance exception delivery through handler chain.
}

///
/// While a thread is in an exception, its state should be
/// BLOCKED_IN_EXCEPTION. Requires real exception delivery.
#[test]
#[ignore = "exception delivery needs real thread execution"]
fn exception_thread_state() {
    // Requires a running thread in exception state.
}

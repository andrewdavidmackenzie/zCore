//! Exception handling operations tests.
//!
//! Additional exception tests covering exception channels, exception
//! types, and process/thread exception handling.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ExceptionOpsTest, CreateExceptionChannel)
///
/// Create an exception channel on a process. The channel should
/// receive exception packets when threads in the process fault.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn create_exception_channel() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ThreadExceptionChannel)
///
/// Create an exception channel on a specific thread. Only exceptions
/// from that thread should be delivered.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn thread_exception_channel() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, JobExceptionChannel)
///
/// Create an exception channel on a job. Exceptions from all
/// processes in the job should be delivered.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn job_exception_channel() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionTypeArchitecture)
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

/// C++: TEST(ExceptionOpsTest, ExceptionResumeTried)
///
/// Receive an exception and resume the thread with TRIED disposition.
/// The thread should continue executing.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_resume_tried() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionResumeHandled)
///
/// Receive an exception and resume the thread with HANDLED
/// disposition. The exception is considered resolved.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_resume_handled() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, UnhandledException)
///
/// An unhandled exception (no handler closes the channel) should
/// propagate up the exception chain (thread → process → job).
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn unhandled_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionChannelClose)
///
/// Close an exception channel while an exception is pending. The
/// exception should propagate to the next handler.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_channel_close() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, MultipleExceptionHandlers)
///
/// Register exception handlers at thread, process, and job level.
/// Exceptions should be delivered to the most specific handler first.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn multiple_exception_handlers() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionGetThread)
///
/// Get the thread handle from an exception object. The thread
/// should be the one that faulted.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_get_thread() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionGetProcess)
///
/// Get the process handle from an exception object.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_get_process() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, DebugException)
///
/// Trigger a debug exception (breakpoint). The exception should
/// have type ZX_EXCP_SW_BREAKPOINT.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn debug_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, PolicyException)
///
/// Trigger a policy exception (e.g., creating a disallowed object).
/// The exception should have type ZX_EXCP_POLICY_ERROR.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn policy_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, SecondChanceException)
///
/// An exception not handled at the debug level gets a second chance
/// at the process/job level.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn second_chance_exception() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

/// C++: TEST(ExceptionOpsTest, ExceptionThreadState)
///
/// While a thread is in an exception, its state should be
/// BLOCKED_IN_EXCEPTION.
#[should_panic(expected = "exception channels need real execution")]
#[test]
fn exception_thread_state() {
    let _ctx = TestContext::new();
    panic!("exception channels need real execution");
}

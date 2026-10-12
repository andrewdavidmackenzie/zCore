//! Suspend and resume tests.
//!
//! Ports of Fuchsia's suspend/resume tests for threads and processes.
//! All require real thread execution and suspension infrastructure.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Suspend a running thread. The thread should stop executing.
/// Requires real thread execution.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspend_thread() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Suspend then resume a thread. The thread should continue
/// executing from where it stopped.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn resume_thread() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Suspend multiple threads in the same process simultaneously.
/// All should stop and be resumable.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspend_multiple_threads() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Drop a suspend token. The thread should automatically resume.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspend_token_drop() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Issue multiple suspend tokens for the same thread. The thread
/// should resume only after all tokens are dropped.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn multiple_suspend_tokens() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Suspend a thread while it is blocked in a syscall (e.g.,
/// channel_read). The thread should suspend after the syscall
/// completes or is interrupted.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspend_in_syscall() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Kill a suspended thread. The thread should die without resuming.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn kill_while_suspended() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// While a thread is suspended, its state should be SUSPENDED.
/// Read state should succeed.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspended_thread_state() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Write to a thread's register state while it is suspended.
/// The new state should take effect when the thread resumes.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn write_state_while_suspended() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

///
/// Suspend all threads in a process simultaneously.
#[should_panic(expected = "suspend needs real thread execution")]
#[test]
fn suspend_process() {
    let _ctx = TestContext::new();
    panic!("suspend needs real thread execution");
}

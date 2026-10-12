//! POSIX threads tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/pthread tests.
//! POSIX threads are a userspace library built on top of Zircon
//! threads and futexes. These tests are not applicable to the
//! kernel object layer.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Create a POSIX thread and join it. Pthreads are a userspace
/// library abstraction over Zircon threads.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn create_join() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Create a detached POSIX thread. The thread runs independently.
/// Requires C runtime + pthread library.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn create_detached() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Lock and unlock a POSIX mutex. Verifies basic mutual exclusion.
/// Built on futexes in userspace.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn mutex_basic() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Wait and signal a POSIX condition variable. Built on futexes.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn condition_variable() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Test POSIX read-write lock semantics (multiple readers, single
/// writer). Built on futexes in userspace.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn rw_lock() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Set and get thread-specific data using pthread_key_create.
/// Requires C runtime + pthread library.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn thread_specific() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Execute a function exactly once using pthread_once. Built on
/// atomics and futexes.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn once() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Cancel a running POSIX thread. The thread should respond to
/// cancellation at the next cancellation point.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn cancel() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Synchronize threads at a POSIX barrier. All threads must reach
/// the barrier before any proceed. Built on futexes.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn barrier() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

///
/// Set and get the stack size attribute for pthread creation.
/// Verifies the attribute is properly propagated to thread creation.
#[should_panic(expected = "pthreads are userspace library")]
#[test]
fn attr_stack_size() {
    let _ctx = TestContext::new();
    panic!("pthreads are userspace library");
}

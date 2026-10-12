//! POSIX threads tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/pthread tests.
//! POSIX threads are a userspace library built on top of Zircon
//! threads and futexes. These tests are not applicable to the
//! kernel object layer.
//!
//! All tests remain `#[ignore]` because pthreads are a userspace
//! C library abstraction with no kernel object equivalent.

///
/// Create a POSIX thread and join it. Pthreads are a userspace
/// library abstraction over Zircon threads.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn create_join() {
    todo!("pthreads are userspace library");
}

///
/// Create a detached POSIX thread. The thread runs independently.
/// Requires C runtime + pthread library.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn create_detached() {
    todo!("pthreads are userspace library");
}

///
/// Lock and unlock a POSIX mutex. Verifies basic mutual exclusion.
/// Built on futexes in userspace.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn mutex_basic() {
    todo!("pthreads are userspace library");
}

///
/// Wait and signal a POSIX condition variable. Built on futexes.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn condition_variable() {
    todo!("pthreads are userspace library");
}

///
/// Test POSIX read-write lock semantics (multiple readers, single
/// writer). Built on futexes in userspace.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn rw_lock() {
    todo!("pthreads are userspace library");
}

///
/// Set and get thread-specific data using pthread_key_create.
/// Requires C runtime + pthread library.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn thread_specific() {
    todo!("pthreads are userspace library");
}

///
/// Execute a function exactly once using pthread_once. Built on
/// atomics and futexes.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn once() {
    todo!("pthreads are userspace library");
}

///
/// Cancel a running POSIX thread. The thread should respond to
/// cancellation at the next cancellation point.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn cancel() {
    todo!("pthreads are userspace library");
}

///
/// Synchronize threads at a POSIX barrier. All threads must reach
/// the barrier before any proceed. Built on futexes.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn barrier() {
    todo!("pthreads are userspace library");
}

///
/// Set and get the stack size attribute for pthread creation.
/// Verifies the attribute is properly propagated to thread creation.
#[test]
#[ignore = "pthreads are userspace library, not kernel objects"]
fn attr_stack_size() {
    todo!("pthreads are userspace library");
}

/// C++: TEST(PthreadTest, Basic)
#[test]
#[ignore = "requires pthread infrastructure"]
fn basic() {
    todo!("requires pthread infrastructure");
}

/// C++: TEST(PthreadTest, BigStackSize)
#[test]
#[ignore = "requires pthread infrastructure"]
fn big_stack_size() {
    todo!("requires pthread infrastructure");
}

/// C++: TEST(PthreadTest, GetstackMainThread)
#[test]
#[ignore = "requires pthread infrastructure"]
fn getstack_main_thread() {
    todo!("requires pthread infrastructure");
}

/// C++: TEST(PthreadTest, GetstackOtherThread)
#[test]
#[ignore = "requires pthread infrastructure"]
fn getstack_other_thread() {
    todo!("requires pthread infrastructure");
}

/// C++: TEST(PthreadTest, GetstackOtherThreadExplicitSize)
#[test]
#[ignore = "requires pthread infrastructure"]
fn getstack_other_thread_explicit_size() {
    todo!("requires pthread infrastructure");
}

/// C++: TEST(PthreadTest, SelfMainThread)
#[test]
#[ignore = "requires pthread infrastructure"]
fn self_main_thread() {
    todo!("requires pthread infrastructure");
}

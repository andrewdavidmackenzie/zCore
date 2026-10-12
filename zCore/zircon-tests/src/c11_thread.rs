//! C11 thread tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/c11-thread tests.
//! C11 threading is a userspace library concern built on top of Zircon
//! futexes and threads. These tests are not applicable to the kernel
//! object layer.
//!
//! All tests remain `#[ignore]` because C11 threads are a userspace
//! library abstraction with no kernel object equivalent.

///
/// Create a C11 thread and join it. C11 threads are a userspace
/// library abstraction over Zircon threads + futexes.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn create_join() {
    todo!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, Detach)
///
/// Create a C11 thread and detach it. The thread runs independently
/// and resources are cleaned up on exit. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn detach() {
    todo!("C11 threads are userspace library");
}

///
/// Create multiple C11 threads and join them all. Verifies thread
/// creation doesn't exhaust resources. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn create_multiple() {
    todo!("C11 threads are userspace library");
}

///
/// Get the current thread ID from within a C11 thread. Verifies
/// thrd_current() returns a valid handle. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn self_id() {
    todo!("C11 threads are userspace library");
}

///
/// Sleep within a C11 thread using thrd_sleep_until. Verifies the
/// sleep duration is approximately correct. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn sleep() {
    todo!("C11 threads are userspace library");
}

///
/// Yield the CPU from a C11 thread using thrd_yield. Verifies the
/// call doesn't crash. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn c11_yield() {
    todo!("C11 threads are userspace library");
}

///
/// Exit a C11 thread with thrd_exit and verify the return code is
/// propagated to the joining thread. Requires C11 runtime.
#[test]
#[ignore = "C11 threads are userspace library, not kernel objects"]
fn exit() {
    todo!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, CreateAndVerifyThreadHandle)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn create_and_verify_thread_handle() {
    todo!("requires C11 thread infrastructure");
}

/// C++: TEST(C11ThreadTest, DetachedThreadKeepsRunning)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn detached_thread_keeps_running() {
    todo!("requires C11 thread infrastructure");
}

/// C++: TEST(C11ThreadTest, LongNameSucceeds)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn long_name_succeeds() {
    todo!("requires C11 thread infrastructure");
}

/// C++: TEST(C11ThreadTest, NullNameThreadShouldSucceed)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn null_name_thread_should_succeed() {
    todo!("requires C11 thread infrastructure");
}

/// C++: TEST(C11ThreadTest, SelfDetachAndFree)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn self_detach_and_free() {
    todo!("requires C11 thread infrastructure");
}

/// C++: TEST(C11ThreadTest, ThreadLocalErrno)
#[test]
#[ignore = "requires C11 thread infrastructure"]
fn thread_local_errno() {
    todo!("requires C11 thread infrastructure");
}

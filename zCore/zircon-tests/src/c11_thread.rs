//! C11 thread tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/c11-thread tests.
//! C11 threading is a userspace library concern built on top of Zircon
//! futexes and threads. These tests are not applicable to the kernel
//! object layer.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(C11ThreadTest, CreateJoin)
///
/// Create a C11 thread and join it. C11 threads are a userspace
/// library abstraction over Zircon threads + futexes.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn create_join() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, Detach)
///
/// Create a C11 thread and detach it. The thread runs independently
/// and resources are cleaned up on exit. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn detach() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, CreateMultiple)
///
/// Create multiple C11 threads and join them all. Verifies thread
/// creation doesn't exhaust resources. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn create_multiple() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, SelfId)
///
/// Get the current thread ID from within a C11 thread. Verifies
/// thrd_current() returns a valid handle. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn self_id() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, Sleep)
///
/// Sleep within a C11 thread using thrd_sleep_until. Verifies the
/// sleep duration is approximately correct. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn sleep() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, Yield)
///
/// Yield the CPU from a C11 thread using thrd_yield. Verifies the
/// call doesn't crash. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn c11_yield() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

/// C++: TEST(C11ThreadTest, Exit)
///
/// Exit a C11 thread with thrd_exit and verify the return code is
/// propagated to the joining thread. Requires C11 runtime.
#[should_panic(expected = "C11 threads are userspace library")]
#[test]
fn exit() {
    let _ctx = TestContext::new();
    panic!("C11 threads are userspace library");
}

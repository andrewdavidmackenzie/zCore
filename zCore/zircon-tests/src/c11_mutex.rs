//! C11 mutex tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/c11-mutex tests.
//! C11 mutexes are a userspace library concern built on top of Zircon
//! futexes. These tests are not applicable to the kernel object layer.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Lock and unlock a C11 mutex. Verifies basic mutual exclusion
/// semantics. C11 mutexes are built on futexes in userspace.
#[should_panic(expected = "C11 mutexes are userspace library")]
#[test]
fn basic_lock_unlock() {
    let _ctx = TestContext::new();
    panic!("C11 mutexes are userspace library");
}

///
/// Try to lock a C11 mutex without blocking. Returns thrd_busy if
/// already held. C11 mutexes are built on futexes in userspace.
#[should_panic(expected = "C11 mutexes are userspace library")]
#[test]
fn try_lock() {
    let _ctx = TestContext::new();
    panic!("C11 mutexes are userspace library");
}

///
/// Lock a C11 mutex with a timeout. Returns thrd_timedout if the
/// lock is not acquired within the deadline. Requires C11 runtime.
#[should_panic(expected = "C11 mutexes are userspace library")]
#[test]
fn timed_lock() {
    let _ctx = TestContext::new();
    panic!("C11 mutexes are userspace library");
}

///
/// Lock a C11 recursive mutex multiple times from the same thread.
/// Each lock must be matched by an unlock. Requires C11 runtime.
#[should_panic(expected = "C11 mutexes are userspace library")]
#[test]
fn recursive() {
    let _ctx = TestContext::new();
    panic!("C11 mutexes are userspace library");
}

///
/// Destroy a C11 mutex. The mutex must not be held when destroyed.
/// Requires C11 runtime.
#[should_panic(expected = "C11 mutexes are userspace library")]
#[test]
fn destroy() {
    let _ctx = TestContext::new();
    panic!("C11 mutexes are userspace library");
}

/// C++: TEST(C11MutexTest, InitalizeLocalMutex)
#[test]
#[ignore = "requires C11 mutex with threads"]
fn initalize_local_mutex() {
    todo!("requires C11 mutex with threads");
}

/// C++: TEST(C11MutexTest, MultiThreadedContention)
#[test]
#[ignore = "requires C11 mutex with threads"]
fn multi_threaded_contention() {
    todo!("requires C11 mutex with threads");
}

/// C++: TEST(C11MutexTest, StaticInitalizerSameBytesAsAuto)
#[test]
#[ignore = "requires C11 mutex with threads"]
fn static_initalizer_same_bytes_as_auto() {
    todo!("requires C11 mutex with threads");
}

/// C++: TEST(C11MutexTest, TimeoutElapsed)
#[test]
#[ignore = "requires C11 mutex with threads"]
fn timeout_elapsed() {
    todo!("requires C11 mutex with threads");
}

/// C++: TEST(C11MutexTest, TryMutexMultiThreadedContention)
#[test]
#[ignore = "requires C11 mutex with threads"]
fn try_mutex_multi_threaded_contention() {
    todo!("requires C11 mutex with threads");
}

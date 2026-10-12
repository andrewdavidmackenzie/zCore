//! Sync mutex tests.
//!
//! Tests for Zircon's futex-based mutex synchronization primitives.
//! These tests require multi-threaded userspace execution with proper
//! scheduling, which is not available in libos test mode. All tests
//! are marked `#[ignore]` to document their expected behavior.

///
/// Basic mutex lock/unlock cycle. A futex-based mutex is created in
/// shared memory, locked by one thread, and unlocked. Requires
/// multi-threaded userspace execution.
#[test]
#[ignore = "needs multi-threaded userspace execution"]
fn sync_mutex_basic_lock_unlock() {
    todo!("needs multi-threaded userspace");
}

///
/// Non-blocking try-lock on a futex-based mutex. If the mutex is
/// unlocked, try-lock succeeds; if locked, it fails immediately
/// without blocking. Requires multi-threaded userspace execution.
#[test]
#[ignore = "needs multi-threaded userspace execution"]
fn sync_mutex_try_lock() {
    todo!("needs multi-threaded userspace");
}

///
/// Multiple threads contend for the same mutex. Only one thread
/// should hold the lock at a time; others block on the futex.
/// Requires multi-threaded userspace execution.
#[test]
#[ignore = "needs multi-threaded userspace execution"]
fn sync_mutex_contended_lock() {
    todo!("needs multi-threaded userspace");
}

///
/// Attempting to lock a non-recursive mutex twice from the same
/// thread should deadlock (or fail in a debug build). This tests
/// that zCore's futex implementation detects the double-lock.
/// Requires multi-threaded userspace execution.
#[test]
#[ignore = "needs multi-threaded userspace execution"]
fn sync_mutex_recursive_fails() {
    todo!("needs multi-threaded userspace");
}

///
/// When a thread that holds a mutex dies (is killed), the kernel
/// should release the mutex (via futex owner tracking). Waiting
/// threads should be woken. Requires multi-threaded userspace execution.
#[test]
#[ignore = "needs multi-threaded userspace execution"]
fn sync_mutex_owner_dies_releases_lock() {
    todo!("needs multi-threaded userspace");
}

/// C++: TEST(SyncMutex, AssertHeld)
#[test]
#[ignore = "requires sync mutex with threads"]
fn assert_held() {
    todo!("requires sync mutex with threads");
}

/// C++: TEST(SyncMutex, Mutexes)
#[test]
#[ignore = "requires sync mutex with threads"]
fn mutexes() {
    todo!("requires sync mutex with threads");
}

/// C++: TEST(SyncMutex, NoRecursion)
#[test]
#[ignore = "requires sync mutex with threads"]
fn no_recursion() {
    todo!("requires sync mutex with threads");
}

/// C++: TEST(SyncMutex, TimeoutElapsed)
#[test]
#[ignore = "requires sync mutex with threads"]
fn timeout_elapsed() {
    todo!("requires sync mutex with threads");
}

/// C++: TEST(SyncMutex, TryMutexes)
#[test]
#[ignore = "requires sync mutex with threads"]
fn try_mutexes() {
    todo!("requires sync mutex with threads");
}

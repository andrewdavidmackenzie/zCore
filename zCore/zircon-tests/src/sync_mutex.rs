//! Sync mutex tests.
//!
//! Tests for Zircon's futex-based mutex synchronization primitives.
//! These tests require multi-threaded userspace execution with proper
//! scheduling, which is not available in libos test mode. All tests
//! are marked `#[should_panic]` to document their expected behavior.

use crate::helpers::TestContext;

extern crate alloc;

///
/// Basic mutex lock/unlock cycle. A futex-based mutex is created in
/// shared memory, locked by one thread, and unlocked. Requires
/// multi-threaded userspace execution.
#[should_panic(expected = "needs multi-threaded userspace")]
#[test]
fn sync_mutex_basic_lock_unlock() {
    let _ctx = TestContext::new();
    panic!("needs multi-threaded userspace");
}

///
/// Non-blocking try-lock on a futex-based mutex. If the mutex is
/// unlocked, try-lock succeeds; if locked, it fails immediately
/// without blocking. Requires multi-threaded userspace execution.
#[should_panic(expected = "needs multi-threaded userspace")]
#[test]
fn sync_mutex_try_lock() {
    let _ctx = TestContext::new();
    panic!("needs multi-threaded userspace");
}

///
/// Multiple threads contend for the same mutex. Only one thread
/// should hold the lock at a time; others block on the futex.
/// Requires multi-threaded userspace execution.
#[should_panic(expected = "needs multi-threaded userspace")]
#[test]
fn sync_mutex_contended_lock() {
    let _ctx = TestContext::new();
    panic!("needs multi-threaded userspace");
}

///
/// Attempting to lock a non-recursive mutex twice from the same
/// thread should deadlock (or fail in a debug build). This tests
/// that zCore's futex implementation detects the double-lock.
/// Requires multi-threaded userspace execution.
#[should_panic(expected = "needs multi-threaded userspace")]
#[test]
fn sync_mutex_recursive_fails() {
    let _ctx = TestContext::new();
    panic!("needs multi-threaded userspace");
}

///
/// When a thread that holds a mutex dies (is killed), the kernel
/// should release the mutex (via futex owner tracking). Waiting
/// threads should be woken. Requires multi-threaded userspace execution.
#[should_panic(expected = "needs multi-threaded userspace")]
#[test]
fn sync_mutex_owner_dies_releases_lock() {
    let _ctx = TestContext::new();
    panic!("needs multi-threaded userspace");
}

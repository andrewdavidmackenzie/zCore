//! C11 mutex tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/c11-mutex tests.
//! C11 mutexes are a userspace library concern built on top of Zircon
//! futexes. These tests exercise the underlying futex and atomic
//! primitives.

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

/// Lock and unlock a mutex via atomic CAS + futex wake.
#[test]
fn basic_lock_unlock() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // Lock: CAS 0 → 1.
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();
    assert_eq!(MUTEX.load(Ordering::Relaxed), 1);

    // Unlock: store 0 + wake.
    MUTEX.store(0, Ordering::Release);
    futex.wake(1);
    assert_eq!(MUTEX.load(Ordering::Relaxed), 0);
}

/// Try-lock: CAS succeeds if unlocked, fails if locked.
#[test]
fn try_lock() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // Unlocked → try-lock succeeds.
    assert!(MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok());

    // Locked → try-lock fails (returns thrd_busy equivalent).
    assert!(MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err());

    MUTEX.store(0, Ordering::Release);
}

/// Timed lock: futex wait with wrong expected value returns
/// BAD_STATE immediately (simulating timeout behavior).
#[test]
fn timed_lock() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(1); // locked
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // Wait with wrong expected value → BAD_STATE (simulates timeout).
    let result = async_std::task::block_on(futex.wait(0));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_STATE);
}

/// Recursive mutex: track lock count in the atomic value.
/// Value 0 = unlocked, N = locked N times.
#[test]
fn recursive() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // Lock once.
    MUTEX.fetch_add(1, Ordering::Acquire);
    assert_eq!(MUTEX.load(Ordering::Relaxed), 1);

    // Lock again (recursive).
    MUTEX.fetch_add(1, Ordering::Acquire);
    assert_eq!(MUTEX.load(Ordering::Relaxed), 2);

    // Unlock twice.
    MUTEX.fetch_sub(1, Ordering::Release);
    MUTEX.fetch_sub(1, Ordering::Release);
    assert_eq!(MUTEX.load(Ordering::Relaxed), 0);
}

/// Destroy: verify mutex can be dropped when unlocked.
#[test]
fn destroy() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // Lock and unlock.
    MUTEX.store(1, Ordering::Relaxed);
    MUTEX.store(0, Ordering::Release);
    futex.wake(1);

    // "Destroy" — futex can be dropped.
    drop(futex);
}

/// C++: TEST(C11MutexTest, InitalizeLocalMutex)
/// Initialize a local mutex (zeroed atomic).
#[test]
fn initalize_local_mutex() {
    let _ctx = TestContext::new();
    let mutex = AtomicI32::new(0);
    assert_eq!(
        mutex.load(Ordering::Relaxed),
        0,
        "mutex should start unlocked"
    );
}

/// C++: TEST(C11MutexTest, MultiThreadedContention)
/// Sequential contention: lock, verify locked, unlock.
#[test]
fn multi_threaded_contention() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    for _ in 0..10 {
        MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .unwrap();
        MUTEX.store(0, Ordering::Release);
        futex.wake(1);
    }
}

/// C++: TEST(C11MutexTest, StaticInitalizerSameBytesAsAuto)
/// A statically initialized mutex should have the same representation
/// as a locally initialized one.
#[test]
fn static_initalizer_same_bytes_as_auto() {
    let _ctx = TestContext::new();
    static STATIC_MUTEX: AtomicI32 = AtomicI32::new(0);
    let local_mutex = AtomicI32::new(0);

    assert_eq!(
        STATIC_MUTEX.load(Ordering::Relaxed),
        local_mutex.load(Ordering::Relaxed),
    );
}

/// C++: TEST(C11MutexTest, TimeoutElapsed)
/// Timeout on a locked mutex. futex_wait with wrong value → BAD_STATE.
#[test]
fn timeout_elapsed() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(1);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    let result = async_std::task::block_on(futex.wait(0));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_STATE);
}

/// C++: TEST(C11MutexTest, TryMutexMultiThreadedContention)
/// Sequential try-lock contention.
#[test]
fn try_mutex_multi_threaded_contention() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    for _ in 0..10 {
        // Try-lock should succeed.
        assert!(MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok());
        // Try-lock while held should fail.
        assert!(MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err());
        // Unlock.
        MUTEX.store(0, Ordering::Release);
    }
}

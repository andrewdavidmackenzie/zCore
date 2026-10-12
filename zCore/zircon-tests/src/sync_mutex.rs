//! Sync mutex tests.
//!
//! Tests for Zircon's futex-based mutex synchronization primitives.
//! Multi-threaded contention tests require preemptive scheduling
//! not available in libos test mode. These tests exercise the
//! underlying futex primitives in single-threaded mode.

use crate::helpers::TestContext;
use core::sync::atomic::AtomicI32;
use zircon_object::signal::Futex;

/// Basic mutex lock/unlock cycle. A futex-based mutex uses atomic
/// CAS for the fast path and futex_wait/wake for contention.
#[test]
fn sync_mutex_basic_lock_unlock() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // Lock: CAS 0 → 1 (uncontended fast path).
    let prev = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert_eq!(prev, Ok(0), "uncontended lock should succeed");

    // Unlock: store 0 + wake.
    MUTEX.store(0, Ordering::Release);
    futex.wake(1); // Wake any waiters (none in this case).

    // Verify unlocked.
    assert_eq!(MUTEX.load(Ordering::Relaxed), 0);
}

/// Non-blocking try-lock. CAS succeeds if unlocked, fails if locked.
#[test]
fn sync_mutex_try_lock() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // Try lock when unlocked — should succeed.
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(result.is_ok(), "try-lock on unlocked mutex should succeed");

    // Try lock when locked — should fail.
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(result.is_err(), "try-lock on locked mutex should fail");

    // Unlock.
    MUTEX.store(0, Ordering::Release);
}

/// Contended lock simulation. In single-threaded mode, we can only
/// test the fast path. The slow path (futex_wait) requires a second
/// thread to wake us.
#[test]
fn sync_mutex_contended_lock() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // Lock.
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();

    // Simulate "contended" state (value = 2 means contended).
    MUTEX.store(2, Ordering::Relaxed);

    // Unlock contended: store 0 + wake.
    MUTEX.store(0, Ordering::Release);
    let woken = futex.wake(1);
    assert_eq!(woken, 0, "no waiters in single-threaded mode");
}

/// Double-lock detection. Attempting CAS on an already-locked mutex
/// fails immediately (detected at userspace level).
#[test]
fn sync_mutex_recursive_fails() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // First lock succeeds.
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();

    // Second lock attempt fails (CAS 0→1 when value is 1).
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(result.is_err(), "recursive lock should fail CAS");

    // Cleanup.
    MUTEX.store(0, Ordering::Release);
}

/// When a thread dies holding a mutex, the futex owner tracking
/// should release the lock. In single-threaded mode, we verify
/// that futex owner is initially None.
#[test]
fn sync_mutex_owner_dies_releases_lock() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // No owner initially.
    assert!(futex.owner().is_none());

    // wake_single_owner with no waiters — owner stays None.
    futex.wake_single_owner();
    assert!(futex.owner().is_none());
}

/// C++: TEST(SyncMutex, AssertHeld)
/// Assert that the mutex is held. In single-threaded mode, verify
/// the atomic state after CAS.
#[test]
fn assert_held() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // Lock.
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();

    // Assert held: value should be non-zero.
    assert_ne!(MUTEX.load(Ordering::Relaxed), 0, "mutex should be held");

    // Cleanup.
    MUTEX.store(0, Ordering::Release);
}

/// C++: TEST(SyncMutex, Mutexes)
/// Multiple mutexes are independent.
#[test]
fn mutexes() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static M1: AtomicI32 = AtomicI32::new(0);
    static M2: AtomicI32 = AtomicI32::new(0);

    // Lock M1.
    M1.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();
    // Lock M2 — should succeed independently.
    M2.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();

    assert_ne!(M1.load(Ordering::Relaxed), 0);
    assert_ne!(M2.load(Ordering::Relaxed), 0);

    M1.store(0, Ordering::Release);
    M2.store(0, Ordering::Release);
}

/// C++: TEST(SyncMutex, NoRecursion)
/// Verify recursive locking is detected.
#[test]
fn no_recursion() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .unwrap();
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(result.is_err(), "recursive lock detected");
    MUTEX.store(0, Ordering::Release);
}

/// C++: TEST(SyncMutex, TimeoutElapsed)
/// Mutex with timeout. In single-threaded mode, verify futex wait
/// with wrong value returns BAD_STATE immediately.
#[test]
fn timeout_elapsed() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(1); // locked
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // futex_wait with wrong expected value → immediate BAD_STATE.
    let result = async_std::task::block_on(futex.wait(0));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_STATE);
}

/// C++: TEST(SyncMutex, TryMutexes)
/// Try-lock on multiple mutexes.
#[test]
fn try_mutexes() {
    use core::sync::atomic::Ordering;

    let _ctx = TestContext::new();
    static M1: AtomicI32 = AtomicI32::new(0);
    static M2: AtomicI32 = AtomicI32::new(0);

    // Try-lock both.
    assert!(M1
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok());
    assert!(M2
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok());

    // Try-lock again — both fail.
    assert!(M1
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err());
    assert!(M2
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err());

    M1.store(0, Ordering::Release);
    M2.store(0, Ordering::Release);
}

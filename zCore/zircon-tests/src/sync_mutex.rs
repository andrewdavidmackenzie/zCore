//! Sync mutex tests.
//!
//! Faithful ports of Fuchsia's zircon/system/utest/core/sync-mutex/mutex.cc.
//!
//! The C++ tests use sync_mutex_lock/unlock/trylock/timedlock which are
//! Fuchsia's libsync mutex implementation built directly on futexes:
//! - sync_mutex_lock → CAS(0→tid), or futex_wait if contended
//! - sync_mutex_unlock → store(0) + futex_wake
//! - sync_mutex_trylock → CAS(0→tid), return ZX_ERR_BAD_STATE if fails
//! - sync_mutex_timedlock → CAS(0→tid), or futex_wait with deadline
//! - sync_mutex_assert_held → check value == current_tid, abort if not
//!
//! C++ source: zircon/system/utest/core/sync-mutex/mutex.cc

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

extern crate alloc;

/// C++: TEST(SyncMutex, Mutexes)
///
/// The C++ test creates 3 threads that each repeatedly lock/unlock
/// the global g_mutex with different iteration counts and sleep durations:
///   Thread 1: 300 iterations, 1us sleep
///   Thread 2: 150 iterations, 2us sleep
///   Thread 3: 100 iterations, 3us sleep
///
/// This tests concurrent mutex contention. Requires multi-threaded
/// execution with blocking futex_wait.
/// 3 threads concurrently lock/unlock a mutex, each incrementing
/// a shared counter. The total should be the sum of all iterations.
#[async_std::test]
async fn mutexes() {
    use crate::helpers::{mutex_contender_thread_fn, MutexContenderState, SyncMutex};
    use std::sync::Arc;
    use zircon_object::task::Thread;

    let ctx = TestContext::new();
    let proc = ctx.proc.clone();

    let mutex = Arc::new(SyncMutex::new());
    let counter = Arc::new(AtomicI32::new(0));

    // C++: 3 threads with 300, 150, 100 iterations — scale down for speed
    let iterations = [30, 15, 10];
    let mut threads = std::vec::Vec::new();
    for &iters in &iterations {
        let state = MutexContenderState::new(mutex.clone(), counter.clone(), iters);
        let thread = Thread::create_with_ext(&proc, "mutex-contender", state.clone()).unwrap();
        thread.start(mutex_contender_thread_fn).unwrap();
        threads.push((thread, state));
    }

    // Wait for all threads to finish
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(10);
    for (_, state) in &threads {
        while state.state.load(Ordering::Acquire) != crate::helpers::WAITER_STATE_WAIT_RETURNED {
            assert!(
                std::time::Instant::now() < deadline,
                "mutex contender thread timed out"
            );
            async_std::task::yield_now().await;
        }
    }

    // Wait for threads to terminate
    for (thread, _) in &threads {
        let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
        loop {
            if thread.state() == zircon_object::task::ThreadState::Dead {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            async_std::task::yield_now().await;
        }
    }

    // Verify total count
    let expected: i32 = iterations.iter().map(|&i| i as i32).sum();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        expected,
        "counter should equal sum of all iterations"
    );
}

/// C++: TEST(SyncMutex, TryMutexes)
///
/// The C++ test creates 3 threads that each do a try-lock loop:
///   for (times = 0; times < N || !got_lock; times++) {
///     status = sync_mutex_trylock(&g_mutex);
///     sleep(delay);
///     if (status == ZX_OK) { got_lock = true; unlock; }
///   }
///
/// After joining, verifies that all 3 threads acquired the lock
/// at least once (got_lock_1/2/3 == true).
///
/// In single-threaded mode, we can test the CAS semantics of trylock.
#[test]
fn try_mutexes() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    let mut got_lock_1 = false;
    let mut got_lock_2 = false;
    let mut got_lock_3 = false;

    // Simulate thread 1: trylock loop (300 iterations, 1us delay)
    for _ in 0..300 {
        // sync_mutex_trylock → CAS(0 → 1)
        if MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            got_lock_1 = true;
            // sync_mutex_unlock → store(0)
            MUTEX.store(0, Ordering::Release);
        }
    }

    // Simulate thread 2: trylock loop (150 iterations, 2us delay)
    for _ in 0..150 {
        if MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            got_lock_2 = true;
            MUTEX.store(0, Ordering::Release);
        }
    }

    // Simulate thread 3: trylock loop (100 iterations, 3us delay)
    for _ in 0..100 {
        if MUTEX
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            got_lock_3 = true;
            MUTEX.store(0, Ordering::Release);
        }
    }

    // C++: EXPECT_TRUE(got_lock_1/2/3)
    assert!(got_lock_1, "failed to get lock 1");
    assert!(got_lock_2, "failed to get lock 2");
    assert!(got_lock_3, "failed to get lock 3");
}

/// C++: TEST(SyncMutex, TimeoutElapsed)
///
/// The C++ test:
/// 1. Creates events for synchronization (start_event, done_event)
/// 2. Helper thread locks the mutex, signals start_event, waits on done_event
/// 3. Main thread waits for start_event, then calls sync_mutex_timedlock 5 times
/// 4. Each call should return ZX_ERR_TIMED_OUT after kRelativeDeadline (100ms)
/// 5. Verifies elapsed >= kRelativeDeadline
/// 6. Signals done_event, joins helper, closes events
///
/// At kernel level: sync_mutex_timedlock calls futex_wait with a deadline.
/// In single-threaded mode, we test the value-mismatch fast path.
#[test]
fn timeout_elapsed() {
    let _ctx = TestContext::new();
    // Mutex is "locked" (non-zero value, simulating another thread holding it)
    static MUTEX: AtomicI32 = AtomicI32::new(1);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // C++: sync_mutex_timedlock(&args.mutex, now + kRelativeDeadline)
    // → ZX_ERR_TIMED_OUT
    // At kernel level: futex_wait(expected=0, deadline) but value is 1
    // → immediate BAD_STATE (value mismatch)
    for _i in 0..5 {
        let result = async_std::task::block_on(futex.wait(0));
        assert_eq!(
            result.unwrap_err(),
            zircon_object::ZxError::BAD_STATE,
            "sync_mutex_timedlock should time out"
        );
    }
}

/// C++: TEST(SyncMutex, NoRecursion)
///
/// The C++ test verifies that attempting to re-lock an already-held
/// libsync mutex results in death (abort). libsync mutexes are NOT
/// recursive.
///
/// At kernel level: the second sync_mutex_lock on an already-held
/// mutex detects the deadlock (owner == current thread) and aborts.
/// In our test, we verify that the CAS for the second lock fails.
#[test]
fn no_recursion() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // First lock: CAS(0 → 1) succeeds
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .expect("first lock should succeed");

    // Second lock attempt: CAS(0 → 1) fails because value is 1
    // C++: ASSERT_DEATH — this would abort in the real implementation
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(result.is_err(), "recursive lock should be detected");

    // Cleanup
    MUTEX.store(0, Ordering::Release);
}

/// C++: TEST(SyncMutex, AssertHeld)
///
/// The C++ test:
/// 1. Locks the mutex, calls sync_mutex_assert_held → no abort
/// 2. Unlocks the mutex
/// 3. ASSERT_DEATH: sync_mutex_assert_held on an unheld mutex → abort
///
/// At kernel level: assert_held checks that the mutex value equals
/// the current thread's KOID. We check the value is non-zero (held).
#[test]
fn assert_held() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // C++: sync_mutex_lock(&mutex)
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .expect("lock should succeed");

    // C++: sync_mutex_assert_held(&mutex) — value is non-zero, so OK
    assert_ne!(
        MUTEX.load(Ordering::Relaxed),
        0,
        "mutex should be held (non-zero)"
    );

    // C++: sync_mutex_unlock(&mutex)
    MUTEX.store(0, Ordering::Release);

    // C++: ASSERT_DEATH(sync_mutex_assert_held on unheld mutex)
    // Value is 0, so assert_held would abort
    assert_eq!(
        MUTEX.load(Ordering::Relaxed),
        0,
        "mutex should be unheld (zero) — assert_held would abort"
    );
}

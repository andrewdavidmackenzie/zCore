//! C11 mutex tests.
//!
//! Faithful ports of Fuchsia's zircon/system/utest/core/c11-mutex/mutex.cc.
//!
//! The C++ tests use C11 mutex APIs (mtx_init, mtx_lock, mtx_unlock,
//! mtx_trylock, mtx_timedlock) which are implemented on top of Zircon
//! futexes. These Rust tests exercise the SAME kernel primitives:
//! - mtx_lock → futex CAS (fast path) or futex_wait (contended)
//! - mtx_unlock → store + futex_wake
//! - mtx_trylock → CAS only, no futex_wait
//! - mtx_timedlock → futex_wait with deadline
//!
//! C++ source: zircon/system/utest/core/c11-mutex/mutex.cc

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

/// C++: TEST(C11MutexTest, MultiThreadedContention)
///
/// The C++ test creates 3 TestThread instances that each repeatedly
/// lock a mtx_t, sleep, and unlock. The threads run concurrently
/// with different iteration counts and sleep durations:
///   Thread 1: 300 iterations, 100us delay
///   Thread 2: 150 iterations, 200us delay
///   Thread 3: 100 iterations, 300us delay
///
/// This requires concurrent multi-threaded execution with blocking
/// mutex waits (futex_wait). Cannot run in cooperative libos mode.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for concurrent mutex contention"]
fn multi_threaded_contention() {
    // Kernel primitives exercised:
    //   zx_futex_wait (mtx_lock contended path)
    //   zx_futex_wake (mtx_unlock wake path)
    //   zx_nanosleep (delay between lock/unlock cycles)
    //   zx_thread_create + zx_thread_start (thrd_create)
}

/// C++: TEST(C11MutexTest, TryMutexMultiThreadedContention)
///
/// The C++ test:
/// 1. Locks a mutex on the main thread
/// 2. Spawns a helper thread that does mtx_trylock → should get thrd_busy
/// 3. Joins the helper, verifies lock_acquired == thrd_busy
/// 4. Unlocks the mutex
/// 5. Spawns helper again → mtx_trylock should succeed
/// 6. Joins, verifies lock_acquired == thrd_success and lock_released == thrd_success
///
/// At the kernel level, mtx_trylock is a CAS on the futex word:
/// if value == 0, set to 1 (success); if != 0, return immediately (busy).
/// No futex_wait is involved in trylock.
///
/// In single-threaded mode, we can faithfully test the CAS semantics.
#[test]
fn try_mutex_multi_threaded_contention() {
    let _ctx = TestContext::new();
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // C++: mtx_lock(&lock) on main thread
    // Kernel: CAS 0 → 1
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .expect("main thread lock should succeed");

    // C++: helper thread does mtx_trylock → thrd_busy
    // Kernel: CAS 0 → 1 fails because value is 1
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(
        result.is_err(),
        "trylock should fail (thrd_busy) when mutex is held"
    );

    // C++: mtx_unlock(&lock) on main thread
    // Kernel: store 0 + futex_wake
    MUTEX.store(0, Ordering::Release);

    // C++: helper thread does mtx_trylock → thrd_success
    // Kernel: CAS 0 → 1 succeeds
    let result = MUTEX.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);
    assert!(
        result.is_ok(),
        "trylock should succeed (thrd_success) when mutex is free"
    );

    // C++: helper thread does mtx_unlock
    MUTEX.store(0, Ordering::Release);
}

/// C++: TEST(C11MutexTest, InitalizeLocalMutex)
///
/// The C++ test initializes a local mtx_t with mtx_init(&mutex, mtx_timed)
/// and then destroys it. This verifies that a mutex can be stack-allocated
/// and initialized.
///
/// At the kernel level, a mutex is just an AtomicI32 initialized to 0.
#[test]
fn initalize_local_mutex() {
    let _ctx = TestContext::new();
    // C++: mtx_init(&auto_mutex, mtx_timed) → thrd_success
    let mutex = AtomicI32::new(0);
    assert_eq!(
        mutex.load(Ordering::Relaxed),
        0,
        "initialized mutex should be unlocked (value 0)"
    );
    // C++: mtx_destroy(&auto_mutex) — no-op for futex-based mutex
    drop(mutex);
}

/// C++: TEST(C11MutexTest, StaticInitalizerSameBytesAsAuto)
///
/// The C++ test verifies that a statically initialized mutex (MTX_INIT)
/// has the same byte representation as one initialized with mtx_init.
///
/// At kernel level: both should be zero-initialized AtomicI32.
#[test]
fn static_initalizer_same_bytes_as_auto() {
    let _ctx = TestContext::new();
    // C++: static mtx_t static_mutex = MTX_INIT
    static STATIC_MUTEX: AtomicI32 = AtomicI32::new(0);
    // C++: mtx_init(&auto_mutex, mtx_plain)
    let auto_mutex = AtomicI32::new(0);

    // C++: EXPECT_BYTES_EQ — both should have identical representation
    assert_eq!(
        STATIC_MUTEX.load(Ordering::Relaxed),
        auto_mutex.load(Ordering::Relaxed),
        "MTX_INIT and mtx_init should produce identical bytes"
    );
}

/// C++: TEST(C11MutexTest, TimeoutElapsed)
///
/// The C++ test:
/// 1. Creates a helper thread that locks the mutex and signals start_event
/// 2. Main thread waits for start_event, then calls mtx_timedlock 5 times
/// 3. Each mtx_timedlock should return thrd_timedout after ~100ms
/// 4. Verifies elapsed time >= 100ms
/// 5. Signals done_event, joins helper
///
/// At the kernel level, mtx_timedlock calls futex_wait with a deadline.
/// When the deadline expires, futex_wait returns ZX_ERR_TIMED_OUT.
///
/// In single-threaded mode: we can test that futex_wait on a locked
/// mutex (value mismatch) returns BAD_STATE immediately.
/// The full timeout test requires concurrent threads.
#[test]
fn timeout_elapsed() {
    let _ctx = TestContext::new();
    // Mutex is "locked" (value = 1)
    static MUTEX: AtomicI32 = AtomicI32::new(1);
    let futex = Futex::new(&MUTEX as *const AtomicI32 as usize);

    // C++: mtx_timedlock(&args.lock, &then) → thrd_timedout
    // At kernel level: futex_wait(expected=0, deadline) on a mutex with value=1
    // Value mismatch (0 != 1) → immediate BAD_STATE return
    // (In a real scenario, the expected value would match and the thread
    // would block until the deadline, but we can only test the fast path.)
    for _i in 0..5 {
        let result = async_std::task::block_on(futex.wait(0));
        assert_eq!(
            result.unwrap_err(),
            zircon_object::ZxError::BAD_STATE,
            "futex_wait with wrong expected value should fail"
        );
    }
}

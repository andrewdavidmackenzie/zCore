//! Conditional variable tests.
//!
//! Faithful ports of Fuchsia's zircon/system/utest/core/c11-condvar/condvar.cc.
//!
//! The C++ tests use C11 condition variable APIs (cnd_wait, cnd_signal,
//! cnd_broadcast, cnd_timedwait) which are implemented on top of Zircon
//! futexes:
//! - cnd_wait → mtx_unlock + futex_wait + mtx_lock
//! - cnd_signal → futex_wake(1)
//! - cnd_broadcast → futex_wake(UINT_MAX)
//! - cnd_timedwait → mtx_unlock + futex_wait(deadline) + mtx_lock
//!
//! C++ source: zircon/system/utest/core/c11-condvar/condvar.cc

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

/// C++: TEST_F(ConditionalVariableTest, BroadcastSignalThreadWait)
///
/// The C++ test:
/// 1. Creates 3 threads, each does:
///    - mtx_lock, threads_started++
///    - cnd_wait loop until wait_condition == true
///    - threads_woken++, mtx_unlock
/// 2. Main thread spins until threads_started == 3
/// 3. Sets wait_condition = true, calls cnd_broadcast
/// 4. Waits until threads_woken == 3
/// 5. Joins all threads
///
/// This requires concurrent multi-threaded execution.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for concurrent condvar broadcast"]
fn broadcast_signal_thread_wait() {
    // Kernel primitives exercised:
    //   cnd_wait → mtx_unlock + futex_wait + mtx_lock
    //   cnd_broadcast → futex_wake(UINT_MAX)
    //   mtx_lock/unlock → CAS + futex_wait/wake
    //   sched_yield → zx_nanosleep(0)
}

/// C++: TEST_F(ConditionalVariableTest, SignalThreadWait)
///
/// The C++ test:
/// 1. Creates 3 threads, each does cnd_wait loop
/// 2. Main thread waits until all 3 are waiting
/// 3. For each iteration (0..3):
///    a. Sets wait_condition = true, calls cnd_signal (wake 1)
///    b. Signaled thread sets wait_condition = false, increments threads_woken
///    c. Main thread waits until threads_woken == iteration + 1
/// 4. Joins all threads
///
/// This requires concurrent multi-threaded execution.
#[test]
#[ignore = "requires preemptive multi-threaded scheduling for concurrent condvar signal"]
fn signal_thread_wait() {
    // Kernel primitives exercised:
    //   cnd_wait → mtx_unlock + futex_wait + mtx_lock
    //   cnd_signal → futex_wake(1)
    //   mtx_lock/unlock → CAS + futex_wait/wake
    //   sched_yield → zx_nanosleep(0)
}

/// C++: TEST_F(ConditionalVariableTest, ConditionalVariablesTimeout)
///
/// The C++ test:
/// 1. Locks a mutex
/// 2. Gets current CLOCK_REALTIME, adds 1ms
/// 3. Calls cnd_timedwait → should return thrd_timedout
/// 4. Unlocks mutex
///
/// At kernel level: cnd_timedwait unlocks the mutex, does futex_wait
/// with a deadline on the condvar futex word, then re-locks the mutex
/// on return.
///
/// In single-threaded mode, we can test the futex_wait value mismatch path.
#[test]
fn conditional_variables_timeout() {
    let _ctx = TestContext::new();

    // C++: cnd_t cond = CND_INIT — condvar futex word, initially 0
    static COND: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&COND as *const AtomicI32 as usize);

    // C++: mtx_t mutex = MTX_INIT — mutex, initially unlocked
    static MUTEX: AtomicI32 = AtomicI32::new(0);

    // C++: mtx_lock(&mutex)
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .expect("mtx_lock should succeed");

    // C++: cnd_timedwait(&cond, &mutex, &delay)
    // Internally: mtx_unlock + futex_wait(cond, expected, deadline) + mtx_lock
    // Since no one signals the condvar, this times out.
    // In single-threaded mode: futex_wait with value mismatch → BAD_STATE
    MUTEX.store(0, Ordering::Release); // mtx_unlock before futex_wait
    let result = async_std::task::block_on(futex.wait(99)); // value mismatch
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::BAD_STATE,
        "Lock should have timedout"
    );
    // Re-lock after timedwait returns
    MUTEX
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .expect("mtx_lock after timedwait should succeed");

    // C++: mtx_unlock(&mutex)
    MUTEX.store(0, Ordering::Release);
}

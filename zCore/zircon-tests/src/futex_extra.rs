//! Additional zCore-specific tests for futex functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::Futex;

/// Futex wake with no waiters returns 0.
fn futex_wake_no_waiters() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    let woken = futex.wake(1);
    assert_eq!(woken, 0);
}

/// Futex wake with count 0 wakes nobody.
fn futex_wake_zero_count() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    let woken = futex.wake(0);
    assert_eq!(woken, 0);
}

/// Futex has a valid koid.
fn futex_koid() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    assert_ne!(futex.id(), 0);
}

/// Regression test for #850: a futex wake racing with FutexFuture's
/// registration sequence must not be lost. The original async host test
/// (removed with async-std in #876) spawned a waiter, yielded once to
/// let it start registering, then changed the value and woke. The waiter
/// must complete (woken or value-changed), never hang.
///
/// Ported to bare metal: spawns a Zircon thread that waits on a futex,
/// yields to let it enter its registration sequence, then changes the
/// value and wakes. The waiter must return — if the wake is lost during
/// registration, the waiter hangs and wait_until_woken times out.
/// Runs 100 iterations to hit the race window.
async fn futex_wake_during_registration_race() {
    use crate::helpers::FutexTestThread;
    use core::sync::atomic::{AtomicI32, Ordering};

    for _iteration in 0..100 {
        let ctx = TestContext::new();
        static VALUE: AtomicI32 = AtomicI32::new(0);
        VALUE.store(1, Ordering::SeqCst);
        let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

        // Spawn a thread that waits on the futex with value 1.
        let waiter = FutexTestThread::start(&ctx.proc, &futex, 1);

        // Wait until the waiter has entered its wait (state =
        // ABOUT_TO_WAIT), then yield once more to give it a chance to
        // reach the registration sequence (the value check + queue
        // push). The exact registration window is internal to
        // FutexFuture::poll and cannot be deterministically targeted;
        // the yield + 100 iterations give many chances to hit it.
        while !waiter.has_wait_returned() && !waiter_is_registering(&waiter) {
            hal_impl::thread::yield_now().await;
        }
        hal_impl::thread::yield_now().await;

        // Change the value and wake. If the waiter is mid-registration,
        // the wake must not be lost (the #850 fix: re-check `woken`
        // after registering in the queue).
        VALUE.store(2, Ordering::SeqCst);
        futex.wake(u32::MAX as usize);

        // The waiter must complete — woken (result 0) or value-changed
        // (result BAD_STATE). If it hangs, the race regressed and
        // wait_until_woken will time out with a clear message.
        waiter.wait_until_woken().await;

        // Shut down the thread (kills it if still alive, waits for
        // termination).
        waiter.shutdown().await;
    }
}

/// Check whether the FutexTestThread's waiter has entered its wait
/// (state = ABOUT_TO_WAIT) but hasn't returned yet. Used by the
/// registration race test to know the waiter has at least started
/// its futex wait before the wake fires.
fn waiter_is_registering(waiter: &crate::helpers::FutexTestThread) -> bool {
    use crate::helpers::WAITER_STATE_ABOUT_TO_WAIT;
    waiter
        .state
        .state
        .load(core::sync::atomic::Ordering::Acquire)
        == WAITER_STATE_ABOUT_TO_WAIT
}

/// Run all tests in this module.
pub async fn run_all() {
    futex_wake_no_waiters();
    futex_wake_zero_count();
    futex_koid();
    futex_wake_during_registration_race().await;
}

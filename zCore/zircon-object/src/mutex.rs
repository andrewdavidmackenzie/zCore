//! Adaptive mutex that spins briefly then yields the CPU.
//!
//! Matches Fuchsia's `CriticalMutex` behavior: attempts a fast CAS
//! for uncontended acquisition, spins for a short time if contended,
//! then yields to the scheduler to let the lock holder run.
//!
//! In libos mode, `sched_yield` is a no-op and this degrades to a
//! spin-lock (the host OS provides preemption). In bare-metal mode,
//! `sched_yield` performs a coroutine context switch, allowing the
//! executor to run the lock holder's task.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Maximum spin iterations before yielding.
/// At ~1ns per iteration, this is ~150µs — matching Fuchsia's
/// `SPIN_MAX_DURATION = ZX_USEC(150)`.
const SPIN_MAX_ITERS: u32 = 150_000;

/// An adaptive mutex: spin briefly, then yield to the scheduler.
pub struct YieldingMutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for YieldingMutex<T> {}
unsafe impl<T: Send> Sync for YieldingMutex<T> {}

impl<T> YieldingMutex<T> {
    /// Create a new unlocked mutex.
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(val),
        }
    }

    /// Acquire the mutex, blocking if necessary.
    ///
    /// Spins for up to `SPIN_MAX_ITERS` iterations. If the lock is
    /// still held, calls `sched_yield()` to let the lock holder run,
    /// then retries.
    pub fn lock(&self) -> YieldingMutexGuard<'_, T> {
        loop {
            // Fast path: try to acquire immediately.
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return YieldingMutexGuard { mutex: self };
            }

            // Spin phase: spin for a bounded time.
            let mut spins = 0u32;
            loop {
                if !self.locked.load(Ordering::Relaxed) {
                    // Lock appears free — try CAS again.
                    break;
                }
                core::hint::spin_loop();
                spins += 1;
                if spins >= SPIN_MAX_ITERS {
                    // Spin budget exhausted — yield to scheduler.
                    hal_impl::thread::sched_yield();
                    break;
                }
            }
        }
    }

    /// Try to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<YieldingMutexGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(YieldingMutexGuard { mutex: self })
        } else {
            None
        }
    }
}

/// RAII guard that releases the mutex on drop.
pub struct YieldingMutexGuard<'a, T> {
    mutex: &'a YieldingMutex<T>,
}

impl<T> Deref for YieldingMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for YieldingMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for YieldingMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

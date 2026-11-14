//! Kernel mutex: adaptive spin-then-yield mutex.
//!
//! This is the standard mutex for all kernel object state. It matches
//! Fuchsia's `Mutex` / `CriticalMutex` semantics:
//!
//! 1. **Fast path**: Uncontended CAS acquire (single atomic).
//! 2. **Spin phase**: Spin for ~150µs with `hint::spin_loop()`.
//! 3. **Yield phase**: Call the registered yield function to let the
//!    lock holder run, then retry.
//!
//! Unlike [`InterruptSpinLock`](crate::Mutex), this mutex does NOT
//! disable interrupts. Interrupt handlers must not acquire a
//! `KernelMutex`.
//!
//! # Lock ordering
//!
//! Each `KernelMutex` has an associated [`LockLevel`]. When the
//! `lockdep` feature is enabled, the runtime checker verifies that
//! locks are acquired in descending level order.
//!
//! # Yield function
//!
//! Call [`set_yield_fn`] during kernel init to register the platform's
//! yield/context-switch function. Without it, the mutex degrades to a
//! pure spinlock (no yielding).

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::lockdep::LockLevel;

/// Maximum spin iterations before yielding.
/// At ~1ns per iteration on a multi-GHz CPU, this is ~150µs — matching
/// Fuchsia's `SPIN_MAX_DURATION = ZX_USEC(150)`.
const SPIN_MAX_ITERS: u32 = 150_000;

// ── Yield function registration ─────────────────────────────────────

/// Stored yield function pointer. 0 = not set (pure spin).
static YIELD_FN: AtomicUsize = AtomicUsize::new(0);

/// Register the platform's yield function.
///
/// This should be called once during kernel initialization, before
/// any `KernelMutex` contention can occur. The function should
/// perform an executor-level context switch (e.g., `sched_yield()`).
///
/// # Safety
///
/// `f` must be a valid function pointer that remains valid for the
/// lifetime of the kernel.
pub fn set_yield_fn(f: fn()) {
    YIELD_FN.store(f as usize, Ordering::Release);
}

/// Call the registered yield function, or spin_loop if none registered.
#[inline]
fn do_yield() {
    let ptr = YIELD_FN.load(Ordering::Acquire);
    if ptr != 0 {
        let f: fn() = unsafe { core::mem::transmute(ptr) };
        f();
    } else {
        // No yield function registered — just hint the CPU.
        core::hint::spin_loop();
    }
}

// ── KernelMutex ─────────────────────────────────────────────────────

/// Adaptive kernel mutex: CAS → spin → yield.
///
/// Does NOT disable interrupts. For interrupt-safe locks, use
/// [`InterruptSpinLock`](crate::Mutex).
pub struct KernelMutex<T: ?Sized> {
    locked: AtomicBool,
    #[cfg_attr(not(feature = "lockdep"), allow(dead_code))]
    level: LockLevel,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for KernelMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for KernelMutex<T> {}

impl<T> KernelMutex<T> {
    /// Create a new unlocked mutex with `Unordered` lock level.
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            level: LockLevel::Unordered,
            data: UnsafeCell::new(val),
        }
    }

    /// Create a new unlocked mutex with a specific lock ordering level.
    pub const fn new_with_level(val: T, level: LockLevel) -> Self {
        Self {
            locked: AtomicBool::new(false),
            level,
            data: UnsafeCell::new(val),
        }
    }

    /// Consume the mutex and return the inner value.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> KernelMutex<T> {
    /// Acquire the mutex, blocking if necessary.
    ///
    /// 1. Fast CAS attempt for uncontended case.
    /// 2. Bounded spin (~150µs) with `hint::spin_loop()`.
    /// 3. Yield to let the lock holder run, then retry.
    ///
    /// Does NOT disable interrupts.
    pub fn lock(&self) -> KernelMutexGuard<'_, T> {
        #[cfg(feature = "lockdep")]
        crate::lockdep::check_lock_order(self.level);

        #[cfg(feature = "lockdep")]
        let mut total_spins: u64 = 0;

        loop {
            // Fast path: try to acquire immediately.
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                #[cfg(feature = "lockdep")]
                crate::lockdep::on_lock_acquired(self.level);
                return KernelMutexGuard { mutex: self };
            }

            // Spin phase: spin for a bounded time.
            let mut spins = 0u32;
            loop {
                if !self.locked.load(Ordering::Relaxed) {
                    break; // Lock appears free — retry CAS.
                }
                core::hint::spin_loop();
                spins += 1;
                #[cfg(feature = "lockdep")]
                {
                    total_spins += 1;
                }
                if spins >= SPIN_MAX_ITERS {
                    do_yield();
                    break;
                }
            }

            #[cfg(feature = "lockdep")]
            if total_spins == 100_000_000 {
                let held = crate::lockdep::held_levels();
                log::warn!(
                    "LOCKDEP: possible deadlock! KernelMutex spinning on {:?} (level {}), \
                     held_levels={:#04x}, total_spins={}",
                    self.level,
                    self.level as u8,
                    held,
                    total_spins,
                );
            }
        }
    }

    /// Try to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<KernelMutexGuard<'_, T>> {
        #[cfg(feature = "lockdep")]
        crate::lockdep::check_lock_order(self.level);

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            #[cfg(feature = "lockdep")]
            crate::lockdep::on_lock_acquired(self.level);
            Some(KernelMutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the underlying data.
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    /// Returns whether the mutex is currently locked.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

// ── Guard ───────────────────────────────────────────────────────────

/// RAII guard that releases the mutex on drop.
pub struct KernelMutexGuard<'a, T: ?Sized> {
    mutex: &'a KernelMutex<T>,
}

impl<T: ?Sized> Deref for KernelMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for KernelMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for KernelMutexGuard<'_, T> {
    fn drop(&mut self) {
        #[cfg(feature = "lockdep")]
        crate::lockdep::on_lock_released(self.mutex.level);
        self.mutex.locked.store(false, Ordering::Release);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for KernelMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "KernelMutex {{ data: {:?} }}", &*guard),
            None => write!(f, "KernelMutex {{ <locked> }}"),
        }
    }
}

impl<T: Default> Default for KernelMutex<T> {
    fn default() -> Self {
        KernelMutex::new(T::default())
    }
}

impl<T> From<T> for KernelMutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

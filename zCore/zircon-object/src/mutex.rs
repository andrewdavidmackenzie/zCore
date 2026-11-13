//! Kernel mutex: adaptive spin-then-yield mutex for kernel object state.
//!
//! This is the standard mutex for all zircon-object kernel state. It
//! matches Fuchsia's `Mutex` / `CriticalMutex` semantics:
//!
//! 1. **Fast path**: Uncontended CAS acquire (single atomic).
//! 2. **Spin phase**: Spin for ~150µs with `hint::spin_loop()`.
//! 3. **Yield phase**: Call `sched_yield()` to let the lock holder run.
//!
//! Unlike `lock::Mutex` (`TicketMutex`/`SpinMutex`), this mutex does
//! NOT disable interrupts. Interrupt handlers must not acquire a
//! `KernelMutex`. Use `lock::Mutex` (InterruptSpinLock) for the few
//! locks accessed from interrupt context (console, timer, allocator).
//!
//! # Lock ordering
//!
//! Each `KernelMutex` has an associated `LockLevel`. The lockdep
//! checker (when the `lockdep` feature is enabled on the `lock` crate)
//! verifies at runtime that locks are acquired in descending level
//! order, catching ABBA deadlocks.
//!
//! # Canonical lock ordering (highest level acquired first)
//!
//! ```text
//! Level 6: Pager::inner
//! Level 5: VmAddressRegion::inner
//! Level 4: VMObjectPaged::lock  (clone-tree-wide)
//! Level 3: VmObject::inner
//! Level 2: VmMapping::inner
//! Level 1: page_table
//! Level 0: unordered (Port, Futex, Thread, Process, Job, etc.)
//! ```

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use lock::lockdep::LockLevel;

/// Maximum spin iterations before yielding.
/// At ~1ns per iteration on a multi-GHz CPU, this is ~150µs — matching
/// Fuchsia's `SPIN_MAX_DURATION = ZX_USEC(150)`.
const SPIN_MAX_ITERS: u32 = 150_000;

/// Adaptive kernel mutex: CAS → spin → yield.
///
/// This is the standard mutex for protecting kernel object state in
/// zircon-object. It does NOT disable interrupts.
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
    /// 3. `sched_yield()` to let the lock holder run, then retry.
    ///
    /// Does NOT disable interrupts.
    pub fn lock(&self) -> KernelMutexGuard<'_, T> {
        #[cfg(feature = "lockdep")]
        lock::lockdep::check_lock_order(self.level);

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
                lock::lockdep::on_lock_acquired(self.level);
                return KernelMutexGuard { mutex: self };
            }

            // Spin phase: spin for a bounded time.
            let mut spins = 0u32;
            loop {
                if !self.locked.load(Ordering::Relaxed) {
                    // Lock appears free — try CAS again (outer loop).
                    break;
                }
                core::hint::spin_loop();
                spins += 1;
                #[cfg(feature = "lockdep")]
                { total_spins += 1; }
                if spins >= SPIN_MAX_ITERS {
                    // Spin budget exhausted — yield to scheduler.
                    hal_impl::thread::sched_yield();
                    break;
                }
            }

            // Deadlock detection: if we've been spinning for a very
            // long time (well beyond any legitimate critical section),
            // warn about a possible deadlock.
            #[cfg(feature = "lockdep")]
            if total_spins == 100_000_000 {
                let held = lock::lockdep::held_levels();
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
    ///
    /// Returns `None` if the mutex is currently held.
    pub fn try_lock(&self) -> Option<KernelMutexGuard<'_, T>> {
        #[cfg(feature = "lockdep")]
        lock::lockdep::check_lock_order(self.level);

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            #[cfg(feature = "lockdep")]
            lock::lockdep::on_lock_acquired(self.level);
            Some(KernelMutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `KernelMutex` mutably, no actual
    /// locking needs to take place — the mutable borrow statically
    /// guarantees no other references exist.
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    /// Returns whether the mutex is currently locked.
    ///
    /// This is a best-effort check — the lock state may change
    /// immediately after this returns.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

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
        lock::lockdep::on_lock_released(self.mutex.level);
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

// Re-export the old name for backward compatibility during migration.
// TODO: remove once all code uses KernelMutex directly.
pub type YieldingMutex<T> = KernelMutex<T>;
pub type YieldingMutexGuard<'a, T> = KernelMutexGuard<'a, T>;

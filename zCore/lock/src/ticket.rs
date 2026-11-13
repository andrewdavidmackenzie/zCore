use core::{
    cell::UnsafeCell,
    default::Default,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::interrupt::{pop_off, push_off};
use crate::lockdep::LockLevel;

/// Interrupt-disabling ticket spinlock.
///
/// Disables interrupts on acquire, restores on drop. Use ONLY for
/// locks that are accessed from interrupt handlers (timer, console,
/// heap allocator). For kernel object state, use `KernelMutex` from
/// `zircon-object::mutex` instead.
///
/// Re-exported as `lock::Mutex` for backward compatibility.
pub struct InterruptSpinLock<T: ?Sized> {
    next_ticket: AtomicUsize,
    next_serving: AtomicUsize,
    /// Lock ordering level for deadlock detection. See `lockdep.rs`.
    level: LockLevel,
    data: UnsafeCell<T>,
}

// Keep old name as alias for any external references.
pub type TicketMutex<T> = InterruptSpinLock<T>;

/// RAII guard for [`InterruptSpinLock`].
///
/// When dropped, the lock is released and the interrupt state
/// saved at acquisition time is restored.
/// RAII guard for [`InterruptSpinLock`].
pub type TicketMutexGuard<'a, T> = InterruptSpinLockGuard<'a, T>;

pub struct InterruptSpinLockGuard<'a, T: ?Sized + 'a> {
    next_serving: &'a AtomicUsize,
    ticket: usize,
    data: &'a mut T,
    /// Whether interrupts were enabled before this lock was acquired.
    irq_was_enabled: bool,
    /// Lock ordering level for tracking.
    level: LockLevel,
}

unsafe impl<T: ?Sized + Send> Sync for TicketMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for TicketMutex<T> {}

impl<T> TicketMutex<T> {
    /// Create a new mutex with `Unordered` lock level (no ordering checks).
    #[inline(always)]
    pub const fn new(data: T) -> Self {
        TicketMutex {
            next_ticket: AtomicUsize::new(0),
            next_serving: AtomicUsize::new(0),
            level: LockLevel::Unordered,
            data: UnsafeCell::new(data),
        }
    }

    /// Create a new mutex with a specific lock ordering level.
    ///
    /// The lock ordering checker will assert at runtime (debug builds)
    /// that this lock is never acquired while holding a lock at the
    /// same or lower level. See `lockdep.rs` for the level hierarchy.
    #[inline(always)]
    pub const fn new_with_level(data: T, level: LockLevel) -> Self {
        TicketMutex {
            next_ticket: AtomicUsize::new(0),
            next_serving: AtomicUsize::new(0),
            level,
            data: UnsafeCell::new(data),
        }
    }

    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    #[inline(always)]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.data.get()
    }
}

impl<T: ?Sized> TicketMutex<T> {
    pub fn lock(&self) -> InterruptSpinLockGuard<'_, T> {
        #[cfg(feature = "lockdep")]
        crate::lockdep::check_lock_order(self.level);
        let irq_was_enabled = push_off();
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "lockdep")]
        {
            /// Spin threshold before emitting a warning. ~100M iterations ≈
            /// several seconds on a multi-GHz CPU — well beyond any
            /// legitimate critical section.
            const SPIN_WARN_THRESHOLD: u64 = 100_000_000;
            let mut spins: u64 = 0;
            while self.next_serving.load(Ordering::Acquire) != ticket {
                core::hint::spin_loop();
                spins += 1;
                if spins == SPIN_WARN_THRESHOLD {
                    let serving = self.next_serving.load(Ordering::Relaxed);
                    let next = self.next_ticket.load(Ordering::Relaxed);
                    let held = crate::lockdep::held_levels();
                    log::warn!(
                        "LOCKDEP: possible deadlock! Spinning on {:?} (level {}), \
                         my_ticket={}, serving={}, next_ticket={}, \
                         held_levels={:#04x}, spins={}",
                        self.level,
                        self.level as u8,
                        ticket,
                        serving,
                        next,
                        held,
                        spins,
                    );
                }
            }
        }
        #[cfg(not(feature = "lockdep"))]
        while self.next_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        #[cfg(feature = "lockdep")]
        crate::lockdep::on_lock_acquired(self.level);
        InterruptSpinLockGuard {
            next_serving: &self.next_serving,
            ticket,
            data: unsafe { &mut *self.data.get() },
            irq_was_enabled,
            level: self.level,
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<InterruptSpinLockGuard<'_, T>> {
        #[cfg(feature = "lockdep")]
        crate::lockdep::check_lock_order(self.level);
        let irq_was_enabled = push_off();
        let ticket = self
            .next_ticket
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |ticket| {
                if self.next_serving.load(Ordering::Acquire) == ticket {
                    Some(ticket + 1)
                } else {
                    None
                }
            });
        if let Ok(ticket) = ticket {
            #[cfg(feature = "lockdep")]
            crate::lockdep::on_lock_acquired(self.level);
            Some(InterruptSpinLockGuard {
                next_serving: &self.next_serving,
                ticket,
                data: unsafe { &mut *self.data.get() },
                irq_was_enabled,
                level: self.level,
            })
        } else {
            pop_off(irq_was_enabled);
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        let ticket = self.next_ticket.load(Ordering::Relaxed);
        self.next_serving.load(Ordering::Relaxed) != ticket
    }
}

impl<'a, T: ?Sized> Drop for InterruptSpinLockGuard<'a, T> {
    fn drop(&mut self) {
        #[cfg(feature = "lockdep")]
        crate::lockdep::on_lock_released(self.level);
        let new_ticket = self.ticket + 1;
        self.next_serving.store(new_ticket, Ordering::Release);
        pop_off(self.irq_was_enabled);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for TicketMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "Mutex {{ data: ")
                .and_then(|()| (&*guard).fmt(f))
                .and_then(|()| write!(f, "}}")),
            None => write!(f, "Mutex {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized + Default> Default for TicketMutex<T> {
    fn default() -> Self {
        TicketMutex::new(T::default())
    }
}

impl<T> From<T> for TicketMutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<'a, T: ?Sized + fmt::Display> fmt::Display for InterruptSpinLockGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for InterruptSpinLockGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for InterruptSpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<'a, T: ?Sized> DerefMut for InterruptSpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

use core::{
    cell::UnsafeCell,
    default::Default,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::interrupt::{pop_off, push_off};

/// Interrupt-disabling ticket spinlock.
///
/// Disables interrupts on acquire, restores on drop. Use ONLY for
/// locks that are accessed from interrupt handlers (timer, console,
/// heap allocator) or that must not be preempted while held. For
/// kernel object state accessed only from task context, use
/// `KernelMutex` instead (see #829).
///
/// Re-exported as `lock::IrqLock` for backward compatibility.
pub struct InterruptSpinLock<T: ?Sized> {
    next_ticket: AtomicUsize,
    next_serving: AtomicUsize,
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
}

unsafe impl<T: ?Sized + Send> Sync for TicketMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for TicketMutex<T> {}

impl<T> TicketMutex<T> {
    /// Create a new mutex.
    #[inline(always)]
    pub const fn new(data: T) -> Self {
        TicketMutex {
            next_ticket: AtomicUsize::new(0),
            next_serving: AtomicUsize::new(0),
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
    #[track_caller]
    pub fn lock(&self) -> InterruptSpinLockGuard<'_, T> {
        let irq_was_enabled = push_off();
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.next_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        InterruptSpinLockGuard {
            next_serving: &self.next_serving,
            ticket,
            data: unsafe { &mut *self.data.get() },
            irq_was_enabled,
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<InterruptSpinLockGuard<'_, T>> {
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
            Some(InterruptSpinLockGuard {
                next_serving: &self.next_serving,
                ticket,
                data: unsafe { &mut *self.data.get() },
                irq_was_enabled,
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

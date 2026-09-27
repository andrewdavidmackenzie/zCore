use core::{
    cell::UnsafeCell,
    default::Default,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::interrupt::{pop_off, push_off};

pub struct TicketMutex<T: ?Sized> {
    next_ticket: AtomicUsize,
    next_serving: AtomicUsize,
    data: UnsafeCell<T>,
}

/// An RAII implementation of a "scoped lock" of a mutex.
/// When this structure is dropped (falls out of scope),
/// the lock will be unlocked and the interrupt state restored.
pub struct TicketMutexGuard<'a, T: ?Sized + 'a> {
    next_serving: &'a AtomicUsize,
    ticket: usize,
    data: &'a mut T,
    /// Whether interrupts were enabled before this lock was acquired.
    irq_was_enabled: bool,
}

unsafe impl<T: ?Sized + Send> Sync for TicketMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for TicketMutex<T> {}

impl<T> TicketMutex<T> {
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
    #[inline(always)]
    pub fn lock(&self) -> TicketMutexGuard<'_, T> {
        let irq_was_enabled = push_off();
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.next_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        TicketMutexGuard {
            next_serving: &self.next_serving,
            ticket,
            data: unsafe { &mut *self.data.get() },
            irq_was_enabled,
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<TicketMutexGuard<'_, T>> {
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
            Some(TicketMutexGuard {
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

impl<'a, T: ?Sized> Drop for TicketMutexGuard<'a, T> {
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

impl<'a, T: ?Sized + fmt::Display> fmt::Display for TicketMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for TicketMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for TicketMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<'a, T: ?Sized> DerefMut for TicketMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

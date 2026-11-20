#![no_std]

//! Low-level lock primitives.
//!
//! # Lock types
//!
//! - [`KernelMutex`] / [`KernelMutexGuard`]: Adaptive spin-then-yield
//!   mutex for kernel object state accessed from task context only.
//!   The holder may be preempted; waiters yield via the registered
//!   yield function (see [`set_yield_fn`]). Interrupt handlers must
//!   NOT acquire it.
//!
//! - [`Mutex`] / [`MutexGuard`]: Interrupt-disabling spinlock (ticket
//!   variant on bare-metal). Use ONLY for locks accessed from
//!   interrupt context (timer, console, heap allocator) or that must
//!   not be preempted while held. Everything else should use
//!   `KernelMutex` — see the lock-ordering work in #829.

pub mod mutex;
pub use mutex::{set_yield_fn, KernelMutex, KernelMutexGuard};

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "none", feature = "ticket"))] {
        extern crate alloc;
        mod interrupt;
        pub mod rwlock;
        pub use rwlock::*;
        pub mod ticket;
        /// Interrupt-disabling spinlock. Disables interrupts on acquire,
        /// restores on drop. Use only for locks accessed from interrupt
        /// handlers (timer, console, allocator) or that must not be
        /// preempted while held. For kernel object state, use
        /// [`KernelMutex`] instead.
        pub use ticket::{InterruptSpinLock as Mutex, InterruptSpinLockGuard as MutexGuard};
    } else if #[cfg(target_os = "none")] {
        extern crate alloc;
        mod interrupt;
        pub mod rwlock;
        pub use rwlock::*;
        pub mod spin;
        /// Interrupt-disabling spinlock. See ticket variant docs.
        pub use spin::{SpinMutex as Mutex, SpinMutexGuard as MutexGuard};
    } else {
        pub use spin::*;
    }
}

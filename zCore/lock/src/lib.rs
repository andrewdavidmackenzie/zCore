#![no_std]

//! Low-level lock primitives.
//!
//! # Lock types
//!
//! - [`Mutex`] / [`MutexGuard`]: Interrupt-disabling spinlock. Use ONLY
//!   for locks accessed from interrupt context (timer, console, heap
//!   allocator). For kernel object state, use `KernelMutex` from the
//!   `zircon-object` crate instead.
//!
//! - [`lockdep`]: Runtime lock ordering checker (behind `lockdep` feature).

pub mod lockdep;
pub mod mutex;
pub use mutex::{set_yield_fn, KernelMutex, KernelMutexGuard};

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "none", feature = "ticket"))] {
        extern crate alloc;
        mod interrupt;
        pub mod mcslock;
        pub mod rwlock;
        pub use {rwlock::*, mcslock::*};
        pub mod ticket;
        /// Interrupt-disabling spinlock. Disables interrupts on acquire,
        /// restores on drop. Use only for locks accessed from interrupt
        /// handlers (timer, console, allocator). For kernel object state,
        /// use `KernelMutex` from `zircon-object::mutex`.
        pub use ticket::{InterruptSpinLock as Mutex, InterruptSpinLockGuard as MutexGuard};
    } else if #[cfg(target_os = "none")] {
        extern crate alloc;
        mod interrupt;
        pub mod mcslock;
        pub mod rwlock;
        pub use {rwlock::*, mcslock::*};
        pub mod spin;
        /// Interrupt-disabling spinlock. See ticket variant docs.
        pub use spin::{SpinMutex as Mutex, SpinMutexGuard as MutexGuard};
    } else {
        pub use spin::*;
    }
}

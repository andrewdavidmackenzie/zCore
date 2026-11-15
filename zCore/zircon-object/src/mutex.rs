//! Re-export kernel mutex from the `lock` crate.
//!
//! `KernelMutex` is the standard mutex for all kernel object state.
//! See [`lock::mutex`] for full documentation.

pub use lock::mutex::{KernelMutex, KernelMutexGuard};

// Backward-compatible aliases.
pub type YieldingMutex<T> = KernelMutex<T>;
pub type YieldingMutexGuard<'a, T> = KernelMutexGuard<'a, T>;

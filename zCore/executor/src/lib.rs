#![no_std]
#![feature(allocator_api)]
#![feature(get_mut_unchecked)]
#![feature(coroutines, coroutine_trait)]
// some interfaces is still under developing
#![allow(dead_code)]

cfg_if::cfg_if! {
  if #[cfg(target_arch = "x86_64")] {
      #[path = "arch/x86_64/mod.rs"]
      #[macro_use]
      mod arch;
  } else if #[cfg(target_arch = "riscv64")] {
      #[path = "arch/riscv64/mod.rs"]
      #[macro_use]
      mod arch;
  } else if #[cfg(target_arch = "aarch64")] {
      #[path = "arch/aarch64/mod.rs"]
      #[macro_use]
      mod arch;
  }
}

extern crate alloc;
#[macro_use]
extern crate log;

cfg_if::cfg_if! {
    if #[cfg(feature = "sched-priority")] {
        #[path = "sched/priority.rs"]
        pub mod sched;
    } else {
        #[path = "sched/cooperative.rs"]
        pub mod sched;
    }
}

mod context;
mod executor;
mod runtime;
pub mod task_collection;
mod waker_page;

/// Flag set by yield_now() to indicate the current task yielded
/// voluntarily. The executor checks this after polling and calls
/// on_yield() instead of relying on the waker self-wake mechanism.
static YIELD_PENDING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Set the yield-pending flag (called from YieldFuture::poll).
pub fn set_yield_pending() {
    YIELD_PENDING.store(true, core::sync::atomic::Ordering::Release);
}

/// Check and clear the yield-pending flag (called from Executor::run).
pub fn take_yield_pending() -> bool {
    YIELD_PENDING.swap(false, core::sync::atomic::Ordering::Acquire)
}

pub use runtime::{
    handle_timeout, init_runtimes, run_until_idle, sched_yield, spawn, spawn_with_priority,
};

#[macro_export]
macro_rules! run_with_intr_saved_on {
    ($($statements:stmt)*) => {
        let enable = crate::arch::intr_get();
        if !enable {
          crate::arch::intr_on();
        }
        $($statements)*
        if !enable {
          crate::arch::intr_off();
        }
    };
}

#[macro_export]
macro_rules! run_with_intr_saved_off {
    ($($statements:stmt)*) => {
        let enable = crate::arch::intr_get();
        if enable {
            crate::arch::intr_off();
        }
        $($statements)*
        if enable {
            crate::arch::intr_on();
        }
    };
}

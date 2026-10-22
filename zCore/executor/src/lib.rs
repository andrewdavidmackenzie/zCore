#![no_std]
#![feature(allocator_api)]
#![feature(get_mut_unchecked)]
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

#[path = "sched/priority.rs"]
pub mod sched;

mod context;
mod executor;
mod runtime;
pub mod task_collection;
mod waker_page;

/// Per-CPU yield-pending flag. Set by PreemptYieldFuture::poll,
/// consumed by Executor::run after polling a task. Using per-CPU
/// storage avoids SMP races where a different CPU's executor could
/// consume the flag.
///
/// Index by CPU ID (max 64 CPUs). On single-CPU systems, only [0] is used.
static YIELD_PENDING: [core::sync::atomic::AtomicBool; 64] = {
    const FALSE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    [FALSE; 64]
};

/// Set the yield-pending flag for the current CPU.
pub fn set_yield_pending() {
    let cpu = arch::cpu_id() as usize;
    if cpu < 64 {
        YIELD_PENDING[cpu].store(true, core::sync::atomic::Ordering::Release);
    }
}

/// Check and clear the yield-pending flag for the current CPU.
pub fn take_yield_pending() -> bool {
    let cpu = arch::cpu_id() as usize;
    if cpu < 64 {
        YIELD_PENDING[cpu].swap(false, core::sync::atomic::Ordering::Acquire)
    } else {
        false
    }
}

pub use runtime::{
    handle_timeout, init_runtimes, run_until_idle, sched_yield, spawn, spawn_with_priority,
};

/// Cross-CPU wakeup callback type.
/// Called with the target CPU ID (APIC ID on x86_64) when a task on
/// a remote CPU is woken. The implementation should send an IPI to
/// break the target CPU out of HLT/WFI.
type WakeupFn = fn(cpu_id: u8);

/// Registered cross-CPU wakeup function. None until set by HAL init.
static WAKEUP_FN: spin::Once<WakeupFn> = spin::Once::new();

/// Register the cross-CPU wakeup function. Called once during HAL
/// initialization after the IPI infrastructure is ready.
pub fn set_ipi_wakeup(f: WakeupFn) {
    WAKEUP_FN.call_once(|| f);
}

/// Send a cross-CPU wakeup if the target CPU differs from the current
/// CPU. Called from `WakerRef::wake_by_ref()`.
pub(crate) fn ipi_wakeup(target_cpu: u8) {
    let current = arch::cpu_id();
    if current != target_cpu {
        if let Some(f) = WAKEUP_FN.get() {
            f(target_cpu);
        }
    }
}

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

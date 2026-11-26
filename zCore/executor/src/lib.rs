#![cfg_attr(not(feature = "libos"), no_std)]
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

// In no_std mode, alloc is an explicit extern crate.
// In std mode (libos), alloc is re-exported from std and
// available implicitly, but `extern crate alloc` is still
// valid and required for uniform `alloc::` paths.
extern crate alloc;
#[macro_use]
extern crate log;

cfg_if::cfg_if! {
    if #[cfg(feature = "sched-fair")] {
        #[path = "sched/fair.rs"]
        pub mod sched;
    } else {
        #[path = "sched/priority.rs"]
        pub mod sched;
    }
}

mod context;
mod executor;
pub mod lock_order;
mod runtime;
pub mod task_collection;
mod waker_page;

// ── Per-CPU scheduler statistics ────────────────────────────────────

use core::sync::atomic::AtomicU64;

/// Per-CPU scheduler statistics, updated atomically.
pub struct CpuSchedStats {
    /// Number of context switches on this CPU.
    pub context_switches: AtomicU64,
    /// Number of times the timer caused a reschedule.
    pub reschedules: AtomicU64,
    /// Number of preemptions (timer interrupt during task execution).
    pub preempts: AtomicU64,
    /// Number of voluntary yields.
    pub yields: AtomicU64,
    /// Number of tasks stolen from other CPUs.
    pub stolen_tasks: AtomicU64,
}

impl CpuSchedStats {
    const fn new() -> Self {
        Self {
            context_switches: AtomicU64::new(0),
            reschedules: AtomicU64::new(0),
            preempts: AtomicU64::new(0),
            yields: AtomicU64::new(0),
            stolen_tasks: AtomicU64::new(0),
        }
    }
}

static CPU_STATS: [CpuSchedStats; 64] = {
    const INIT: CpuSchedStats = CpuSchedStats::new();
    [INIT; 64]
};

/// Get the scheduler stats for a CPU.
pub fn cpu_stats(cpu_id: usize) -> &'static CpuSchedStats {
    &CPU_STATS[cpu_id.min(63)]
}

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

/// Per-CPU preemption-pending flag.
///
/// Set by the timer interrupt handler to indicate the current task's
/// time quantum has expired. The syscall return path checks this flag
/// and yields only when set, matching Fuchsia's lazy preemption model
/// where syscalls return directly to userspace unless a timer interrupt
/// has requested a reschedule.
static PREEMPT_PENDING: [core::sync::atomic::AtomicBool; 64] = {
    const FALSE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    [FALSE; 64]
};

/// Set preemption-pending for the current CPU (called from timer IRQ).
pub fn set_preempt_pending() {
    let cpu = arch::cpu_id() as usize;
    if cpu < 64 {
        PREEMPT_PENDING[cpu].store(true, core::sync::atomic::Ordering::Release);
    }
}

/// Check and clear preemption-pending for the current CPU.
pub fn take_preempt_pending() -> bool {
    let cpu = arch::cpu_id() as usize;
    if cpu < 64 {
        PREEMPT_PENDING[cpu].swap(false, core::sync::atomic::Ordering::Acquire)
    } else {
        false
    }
}

pub use runtime::{
    abandon_current_poll, change_task_priority, force_wake_task, handle_timeout, init_runtimes,
    request_shutdown, run_until_idle, sched_yield, set_deferred_drain_fn, shutdown_requested,
    spawn, spawn_with_priority,
};

/// Save the kernel page table address for executor context creation.
/// Must be called once during boot before any timer preemption can
/// occur. On x86_64, this prevents new executors from inheriting a
/// user page table via CR3 during preemption.
#[cfg(target_arch = "x86_64")]
pub fn set_kernel_page_table(phys: usize) {
    arch::set_kernel_page_table(phys);
}

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

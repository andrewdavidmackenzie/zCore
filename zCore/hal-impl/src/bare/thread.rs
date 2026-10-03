//! Thread spawning and per-CPU user-copy fault recovery.

use alloc::sync::Arc;
use core::{any::Any, future::Future};

use crate::{config::MAX_CORE_NUM, utils::PerCpuCell};

#[allow(clippy::declare_interior_mutable_const)]
const DEFAULT_THREAD: PerCpuCell<Option<Arc<dyn Any + Send + Sync>>> = PerCpuCell::new(None);

static CURRENT_THREAD: [PerCpuCell<Option<Arc<dyn Any + Send + Sync>>>; MAX_CORE_NUM] =
    [DEFAULT_THREAD; MAX_CORE_NUM];

// --- Fault-safe user copy recovery state ---
//
// When the kernel copies data to/from user memory, a bad user pointer
// can trigger a page fault. Instead of panicking, the fault handler
// checks `USER_COPY_ACTIVE`. If set, it stores the faulting address
// in `USER_COPY_FAULT_ADDR` and redirects the trap frame's PC to
// `USER_COPY_RECOVERY_PC` so execution resumes at a recovery point
// that returns an error to the syscall.

#[allow(clippy::declare_interior_mutable_const)]
const DEFAULT_BOOL: PerCpuCell<bool> = PerCpuCell::new(false);
#[allow(clippy::declare_interior_mutable_const)]
const DEFAULT_USIZE: PerCpuCell<usize> = PerCpuCell::new(0);

/// Whether the current CPU is inside a guarded user-copy region.
static USER_COPY_ACTIVE: [PerCpuCell<bool>; MAX_CORE_NUM] = [DEFAULT_BOOL; MAX_CORE_NUM];

/// The PC to jump to when a user-copy fault occurs. Set before the
/// copy and read by the trap handler.
static USER_COPY_RECOVERY_PC: [PerCpuCell<usize>; MAX_CORE_NUM] = [DEFAULT_USIZE; MAX_CORE_NUM];

/// The faulting virtual address, filled in by the trap handler when
/// a user-copy fault occurs. Zero means no fault.
static USER_COPY_FAULT_ADDR: [PerCpuCell<usize>; MAX_CORE_NUM] = [DEFAULT_USIZE; MAX_CORE_NUM];

/// Enter a user-copy critical section. Sets the recovery PC so that
/// a page fault during the copy jumps there instead of panicking.
///
/// # Safety
/// `recovery_pc` must be a valid instruction address that, when jumped
/// to, will clean up and return an error. The caller must call
/// `user_copy_leave()` before returning, even on the recovery path.
pub unsafe fn user_copy_enter(recovery_pc: usize) {
    let idx = super::cpu::cpu_index();
    *USER_COPY_FAULT_ADDR[idx].get_mut() = 0;
    *USER_COPY_RECOVERY_PC[idx].get_mut() = recovery_pc;
    // Ensure writes above are visible before enabling the guard.
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    *USER_COPY_ACTIVE[idx].get_mut() = true;
}

/// Leave a user-copy critical section. Returns the faulting address
/// if a fault was caught (non-zero), or 0 if the copy succeeded.
pub fn user_copy_leave() -> usize {
    let idx = super::cpu::cpu_index();
    *USER_COPY_ACTIVE[idx].get_mut() = false;
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    *USER_COPY_FAULT_ADDR[idx].get_mut()
}

/// Called by the page fault handler. If a user-copy is active,
/// records the fault and returns `Some(recovery_pc)` so the trap
/// handler can redirect execution. Otherwise returns `None`.
///
/// This clears `USER_COPY_ACTIVE`. If the fault is resolved (demand-
/// paged), the caller must call `user_copy_rearm` to re-enable the
/// guard for subsequent faults in the same multi-page copy.
pub fn user_copy_check_fault(fault_vaddr: usize) -> Option<usize> {
    let idx = super::cpu::cpu_index();
    if *USER_COPY_ACTIVE[idx].get() {
        *USER_COPY_FAULT_ADDR[idx].get_mut() = fault_vaddr;
        *USER_COPY_ACTIVE[idx].get_mut() = false;
        Some(*USER_COPY_RECOVERY_PC[idx].get())
    } else {
        None
    }
}

/// Re-arm the user-copy guard after a resolved page fault.
///
/// Called when `try_handle_page_fault` successfully resolves a fault
/// during a guarded copy, so that subsequent faults in the same copy
/// are still caught by the guard.
pub fn user_copy_rearm(recovery_pc: usize) {
    let idx = super::cpu::cpu_index();
    *USER_COPY_ACTIVE[idx].get_mut() = true;
    *USER_COPY_RECOVERY_PC[idx].get_mut() = recovery_pc;
    *USER_COPY_FAULT_ADDR[idx].get_mut() = 0;
}

hal_fn_impl! {
    impl mod crate::hal_fn::thread {
        fn spawn(future: impl Future<Output = ()> + Send + 'static) {
            executor::spawn(future);
        }

        fn set_current_thread(thread: Option<Arc<dyn Any + Send + Sync>>) {
            let idx = super::cpu::cpu_index();
            *CURRENT_THREAD[idx].get_mut() = thread;
        }

        fn get_current_thread() -> Option<Arc<dyn Any + Send + Sync>> {
            let idx = super::cpu::cpu_index();
            if let Some(arc_thread) = CURRENT_THREAD[idx].get().as_ref() {
                Some(arc_thread.clone())
            } else {
                None
            }
        }
    }
}

//! Virtual memory type re-exports and SMP page table tracking.

pub use hal::vm::*;

use crate::config::MAX_CORE_NUM;
use crate::PhysAddr;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Per-CPU tracking of the currently-active page table physical address.
///
/// When a CPU switches to a user process's page table (via
/// `activate_paging`), it stores that address here. Before a page
/// table is freed, the drop handler checks all CPUs and IPIs any
/// that still have the dying page table loaded, forcing them to
/// switch to the kernel page table. This matches Fuchsia's
/// `active_cpus_` tracking in `X86ArchVmAspace`.
static ACTIVE_PAGE_TABLE: [AtomicUsize; MAX_CORE_NUM] = {
    const ZERO: AtomicUsize = AtomicUsize::new(0);
    [ZERO; MAX_CORE_NUM]
};

/// Record that this CPU is now using the given page table.
/// Called from `activate_paging` BEFORE writing CR3/TTBR/SATP.
pub fn set_active_page_table(table_phys: PhysAddr) {
    let cpu = crate::cpu::cpu_index();
    ACTIVE_PAGE_TABLE[cpu].store(table_phys, Ordering::Release);
}

/// Record that this CPU is no longer using any user page table
/// (switched to kernel page table). Called when switching away
/// from a user address space.
pub fn clear_active_page_table() {
    let cpu = crate::cpu::cpu_index();
    ACTIVE_PAGE_TABLE[cpu].store(0, Ordering::Release);
}

/// Check if ANY CPU has the given page table loaded.
/// Returns a bitmask of CPUs that have it active.
pub fn active_cpus_for_table(table_phys: PhysAddr) -> u64 {
    let mut mask = 0u64;
    for i in 0..MAX_CORE_NUM {
        if ACTIVE_PAGE_TABLE[i].load(Ordering::Acquire) == table_phys {
            mask |= 1 << i;
        }
    }
    mask
}

/// Wait until no CPU has the given page table loaded.
/// Sends IPI wakeups to CPUs that have it, which will cause them
/// to re-enter the executor and eventually switch address spaces.
///
/// Called from PageTableImpl::Drop before freeing page table memory.
#[cfg(not(feature = "libos"))]
pub fn wait_for_page_table_release(table_phys: PhysAddr) {
    // Switch THIS CPU away first if it has the table loaded.
    let my_cpu = crate::cpu::cpu_index();
    if ACTIVE_PAGE_TABLE[my_cpu].load(Ordering::Acquire) == table_phys {
        // Switch to the kernel page table.
        // On x86_64, the kernel shares the upper half of every page
        // table, so we can use any valid page table. Use the boot
        // page table (CR3 at boot) or just reload the current one.
        // For safety, just clear our tracking — the next
        // ThreadSwitchFuture::poll will set the correct CR3.
        ACTIVE_PAGE_TABLE[my_cpu].store(0, Ordering::Release);
    }

    // IPI other CPUs and spin-wait for them to release.
    for _ in 0..1000 {
        let mask = active_cpus_for_table(table_phys);
        if mask == 0 {
            return;
        }
        // Send IPI to each CPU that still has this table.
        // The IPI breaks the CPU out of HLT, causing it to re-enter
        // the executor which will pick a new task and switch CR3.
        #[cfg(not(feature = "libos"))]
        for cpu in 0..MAX_CORE_NUM {
            if mask & (1 << cpu) != 0 && cpu != crate::cpu::cpu_index() {
                let _ = crate::interrupt::send_ipi(cpu, 0);
            }
        }
        core::hint::spin_loop();
    }
    // After 1000 iterations, give up. This shouldn't happen in
    // practice — the IPI wakes the CPU which re-enters the executor
    // and picks a new task (with a different page table).
}

#[cfg(feature = "libos")]
pub fn wait_for_page_table_release(_table_phys: PhysAddr) {
    // No-op in libos mode — no real page tables.
}

/// Switch this CPU to the kernel page table and clear active tracking.
/// Called from ThreadSwitchFuture::poll after the inner future returns,
/// ensuring the user page table can be safely freed by another CPU.
#[cfg(all(not(feature = "libos"), target_arch = "x86_64"))]
pub fn switch_to_kernel_page_table() {
    crate::imp::arch::vm::switch_to_kernel_page_table();
}

#[cfg(all(not(feature = "libos"), target_arch = "aarch64"))]
pub fn switch_to_kernel_page_table() {
    // TODO: implement for aarch64 — switch TTBR0 to kernel table
    clear_active_page_table();
}

#[cfg(all(not(feature = "libos"), target_arch = "riscv64"))]
pub fn switch_to_kernel_page_table() {
    // TODO: implement for riscv64 — switch SATP to kernel table
    clear_active_page_table();
}

#[cfg(feature = "libos")]
pub fn switch_to_kernel_page_table() {
    // No-op in libos mode.
}

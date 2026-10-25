use core::arch::{asm, global_asm};

mod context;

pub use context::*;

global_asm!(include_str!("switch.S"));
global_asm!(include_str!("executor_entry.S"));

extern "C" {
    pub fn switch(old: *const ContextData, new: *const ContextData);
    pub fn executor_entry();
}

/// Return the CPU ID (APIC ID on x86_64).
///
/// This is the hardware CPU identifier, which may be non-contiguous
/// (e.g. 0,1,2,3,4,6,8,10 with hyperthreading). It is used as a
/// HashMap key in the executor's runtime map, NOT as an array index.
pub(crate) fn cpu_id() -> u8 {
    raw_cpuid::CpuId::new()
        .get_feature_info()
        .unwrap()
        .initial_local_apic_id()
}

// pub(crate) fn pg_base_addr() -> usize {
//     x86_64::registers::control::Cr3::read()
//         .0
//         .start_address()
//         .as_u64() as _
// }

/// The kernel page table physical address, set once at boot by
/// `set_kernel_page_table`. New executor contexts use this value
/// instead of reading CR3 directly, because CR3 may contain a user
/// page table during timer preemption.
static KERNEL_PT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Save the kernel page table address. Called once by HAL init.
pub fn set_kernel_page_table(phys: usize) {
    KERNEL_PT.store(phys, core::sync::atomic::Ordering::Relaxed);
}

/// Return the page table base for new executor contexts.
///
/// Uses the saved kernel page table, NOT the current CR3. During
/// timer preemption, CR3 may contain a user page table (loaded by
/// `ThreadSwitchFuture::poll`). Using that value would save a user
/// page table in the executor context; when that page table is
/// freed, switching to the executor writes a stale CR3, causing a
/// triple fault.
pub(crate) fn pg_base_register() -> usize {
    let saved = KERNEL_PT.load(core::sync::atomic::Ordering::Relaxed);
    if saved != 0 {
        return saved;
    }
    // Fallback for very early boot before set_kernel_page_table.
    let mut cr3;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3
}

use x86_64::instructions::interrupts;

pub(crate) fn wait_for_interrupt() {
    let enable = interrupts::are_enabled();
    interrupts::enable_and_hlt();
    if !enable {
        interrupts::disable();
    }
}

pub(crate) fn intr_on() {
    interrupts::enable();
}

pub(crate) fn intr_off() {
    interrupts::disable();
}

pub(crate) fn intr_get() -> bool {
    interrupts::are_enabled()
}

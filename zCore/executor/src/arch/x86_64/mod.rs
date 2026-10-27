use core::arch::global_asm;

mod context;

pub use context::*;

// Use the libos switch assembly (no CR3) or the bare-metal one.
#[cfg(feature = "libos")]
global_asm!(include_str!("switch_libos.S"));
#[cfg(not(feature = "libos"))]
global_asm!(include_str!("switch.S"));

global_asm!(include_str!("executor_entry.S"));

extern "C" {
    pub fn switch(old: *const ContextData, new: *const ContextData);
    pub fn executor_entry();
}

// ── LibOS mode: host-OS compatible implementations ──────────────────

#[cfg(feature = "libos")]
pub(crate) fn cpu_id() -> u8 {
    0 // single CPU in libos mode
}

#[cfg(feature = "libos")]
pub(crate) fn pg_base_register() -> usize {
    0 // no page tables in libos mode
}

#[cfg(feature = "libos")]
pub fn set_kernel_page_table(_phys: usize) {
    // no-op in libos mode
}

#[cfg(feature = "libos")]
pub(crate) fn wait_for_interrupt() {
    // Yield to the host OS scheduler instead of HLT
    std::thread::yield_now();
}

#[cfg(feature = "libos")]
pub(crate) fn intr_on() {
    // no-op — host OS manages interrupts
}

#[cfg(feature = "libos")]
pub(crate) fn intr_off() {
    // no-op — host OS manages interrupts
}

#[cfg(feature = "libos")]
pub(crate) fn intr_get() -> bool {
    true // interrupts are always "enabled" in libos
}

// ── Bare-metal mode: hardware implementations ───────────────────────

#[cfg(not(feature = "libos"))]
pub(crate) fn cpu_id() -> u8 {
    raw_cpuid::CpuId::new()
        .get_feature_info()
        .unwrap()
        .initial_local_apic_id()
}

#[cfg(not(feature = "libos"))]
static KERNEL_PT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

#[cfg(not(feature = "libos"))]
pub fn set_kernel_page_table(phys: usize) {
    KERNEL_PT.store(phys, core::sync::atomic::Ordering::Relaxed);
}

#[cfg(not(feature = "libos"))]
pub(crate) fn pg_base_register() -> usize {
    let saved = KERNEL_PT.load(core::sync::atomic::Ordering::Relaxed);
    if saved != 0 {
        return saved;
    }
    let mut cr3;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3
}

#[cfg(not(feature = "libos"))]
pub(crate) fn wait_for_interrupt() {
    use x86_64::instructions::interrupts;
    let enable = interrupts::are_enabled();
    interrupts::enable_and_hlt();
    if !enable {
        interrupts::disable();
    }
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_on() {
    x86_64::instructions::interrupts::enable();
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_off() {
    x86_64::instructions::interrupts::disable();
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_get() -> bool {
    x86_64::instructions::interrupts::are_enabled()
}

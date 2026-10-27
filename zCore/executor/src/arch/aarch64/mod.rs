use core::arch::global_asm;

mod context;

pub use context::*;

// Use the libos switch assembly (no TTBR0) or the bare-metal one.
#[cfg(feature = "libos")]
global_asm!(include_str!("switch_libos.S"));
#[cfg(not(feature = "libos"))]
global_asm!(include_str!("switch.S"));

#[cfg(not(feature = "libos"))]
global_asm!(include_str!("executor_entry.S"));

// In libos mode, define executor_entry as a Rust function to avoid
// assembly symbol naming differences between macOS (_-prefix) and Linux.
#[cfg(feature = "libos")]
#[no_mangle]
pub unsafe extern "C" fn executor_entry() {
    // Read the executor address from the bottom of the stack.
    // The stack was set up by init_stack_and_context to have the
    // executor address at the bottom.
    let executor_addr: usize;
    core::arch::asm!(
        "ldr {}, [sp]",
        out(reg) executor_addr,
        options(nomem, nostack),
    );
    crate::runtime::run_executor(executor_addr);
}

extern "C" {
    pub fn switch(old: *const ContextData, new: *const ContextData);
    #[cfg(not(feature = "libos"))]
    pub fn executor_entry();
}

// ── LibOS mode: host-OS compatible implementations ──────────────────

#[cfg(feature = "libos")]
pub(crate) fn cpu_id() -> u8 {
    0
}

#[cfg(feature = "libos")]
pub(crate) fn pg_base_register() -> usize {
    0
}

#[cfg(feature = "libos")]
pub(crate) fn wait_for_interrupt() {
    std::thread::yield_now();
}

#[cfg(feature = "libos")]
pub(crate) fn intr_on() {}

#[cfg(feature = "libos")]
pub(crate) fn intr_off() {}

#[cfg(feature = "libos")]
pub(crate) fn intr_get() -> bool {
    true
}

// ── Bare-metal mode: hardware implementations ───────────────────────

#[cfg(not(feature = "libos"))]
use cortex_a::registers::*;
#[cfg(not(feature = "libos"))]
use tock_registers::interfaces::Readable;

#[cfg(not(feature = "libos"))]
pub(crate) fn cpu_id() -> u8 {
    (MPIDR_EL1.get() & 0xf) as u8
}

#[cfg(not(feature = "libos"))]
pub(crate) fn pg_base_addr() -> usize {
    TTBR0_EL1.get() as usize
}

#[cfg(not(feature = "libos"))]
pub(crate) fn pg_base_register() -> usize {
    TTBR0_EL1.get() as usize
}

#[cfg(not(feature = "libos"))]
pub(crate) fn wait_for_interrupt() {
    let enable = intr_get();
    if !enable {
        intr_on();
    }
    cortex_a::asm::wfi();
    if !enable {
        intr_off();
    }
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_on() {
    unsafe {
        core::arch::asm!("msr daifclr, #2", "isb");
    }
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_off() {
    unsafe {
        core::arch::asm!("msr daifset, #2", "isb");
    }
}

#[cfg(not(feature = "libos"))]
pub(crate) fn intr_get() -> bool {
    !DAIF.is_set(DAIF::I)
}

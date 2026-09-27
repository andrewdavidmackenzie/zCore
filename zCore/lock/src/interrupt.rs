//! Interrupt management for spin locks.
//!
//! Provides `push_off`/`pop_off` to disable/restore interrupts around
//! critical sections. Each `push_off` returns a `bool` indicating whether
//! interrupts were enabled before the call. The corresponding `pop_off`
//! restores that state. This handles nesting naturally without any
//! per-CPU state or CPU ID lookups.

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "none", any(target_arch = "riscv32", target_arch = "riscv64")))] {
        mod interrupts {
            pub(crate) fn intr_on() {
                unsafe { riscv::register::sstatus::set_sie() };
            }
            pub(crate) fn intr_off() {
                unsafe { riscv::register::sstatus::clear_sie() };
            }
            pub(crate) fn intr_get() -> bool {
                riscv::register::sstatus::read().sie()
            }
        }
    } else if #[cfg(all(target_os = "none", any(target_arch = "x86", target_arch = "x86_64")))] {
        mod interrupts {
            use x86_64::instructions::interrupts;
            pub(crate) fn intr_on() {
                interrupts::enable();
            }
            pub(crate) fn intr_off() {
                interrupts::disable();
            }
            pub(crate) fn intr_get() -> bool {
                interrupts::are_enabled()
            }
        }
    } else if #[cfg(all(target_os = "none", target_arch = "aarch64"))] {
        mod interrupts {
            pub(crate) fn intr_on() {
                unsafe {
                    core::arch::asm!("msr daifclr, #2");
                }
            }
            pub(crate) fn intr_off() {
                unsafe {
                    core::arch::asm!("msr daifset, #2");
                }
            }
            pub(crate) fn intr_get() -> bool {
                use cortex_a::registers::DAIF;
                use tock_registers::interfaces::Readable;
                !DAIF.is_set(DAIF::I)
            }
        }
    } else {
        mod interrupts {
            pub(crate) fn intr_on() { unimplemented!(); }
            pub(crate) fn intr_off() { unimplemented!(); }
            pub(crate) fn intr_get() -> bool { unimplemented!(); }
        }
    }
}

use interrupts::*;

/// Disable interrupts and return whether they were previously enabled.
///
/// Each lock guard stores this return value and passes it to `pop_off`
/// on drop. Nesting is handled naturally: inner locks see
/// `was_enabled = false` (already off) and their `pop_off` is a no-op.
#[inline(always)]
pub(crate) fn push_off() -> bool {
    let was_enabled = intr_get();
    intr_off();
    was_enabled
}

/// Restore the interrupt state saved by a previous `push_off`.
#[inline(always)]
pub(crate) fn pop_off(was_enabled: bool) {
    if was_enabled {
        intr_on();
    }
}

//! Interrupt management for spin locks.
//!
//! Provides `push_off`/`pop_off` to disable/restore interrupts around
//! critical sections. Each `push_off` returns a `bool` indicating whether
//! interrupts were enabled before the call. The corresponding `pop_off`
//! restores that state.
//!
//! **Guards must be dropped in LIFO (stack) order.** Dropping an outer
//! guard before an inner guard would re-enable interrupts while a lock
//! is still held, allowing interrupt handlers to deadlock. This is
//! enforced by a debug assertion in `pop_off`.

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
///
/// Guards must be dropped in LIFO order. Dropping an outer guard first
/// would re-enable interrupts while an inner lock is still held.
#[inline(always)]
pub(crate) fn push_off() -> bool {
    let was_enabled = intr_get();
    intr_off();
    was_enabled
}

/// Restore the interrupt state saved by a previous `push_off`.
///
/// # Safety invariant
///
/// If `was_enabled` is true, interrupts must actually be safe to
/// re-enable (i.e., no other interrupt-disabling lock is held).
/// This is guaranteed when guards are dropped in LIFO order.
#[inline(always)]
pub(crate) fn pop_off(was_enabled: bool) {
    // If was_enabled is true, we're about to turn interrupts on.
    // At this point interrupts should still be off (we haven't
    // re-enabled yet). If they're already on, a guard was dropped
    // out of order — an inner lock re-enabled interrupts before
    // the outer lock was released.
    debug_assert!(
        !was_enabled || !intr_get(),
        "pop_off: interrupts already enabled — guards dropped out of LIFO order"
    );
    if was_enabled {
        intr_on();
    }
}

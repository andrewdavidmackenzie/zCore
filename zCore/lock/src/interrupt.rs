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

/// Global count of live interrupt-disabling guards (IrqLock critical
/// sections) across all CPUs. Only reaches 0 when no CPU holds one.
/// See `push_off` for the rationale.
///
/// On x86_64 bare-metal this is only used before
/// [`set_gs_depth_ready`] is called (BSP early boot, before the
/// per-CPU TSS is installed as the GS base); afterwards the per-CPU
/// depth in `TSS.rsp2` takes over. Other architectures use it always.
pub(crate) static IRQ_LOCK_DEPTH: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Whether an IrqLock critical section is held on THIS CPU.
///
/// The `KernelMutex` contended path uses this to decide whether
/// yielding is safe: a critical section held by ANOTHER CPU does not
/// prevent this CPU from yielding (the normal spin-then-yield
/// behavior waits for it), but one held by THIS CPU must never be
/// abandoned mid-guard (#862).
#[cfg(not(all(target_os = "none", target_arch = "x86_64")))]
pub fn irq_lock_held_on_this_cpu() -> bool {
    IRQ_LOCK_DEPTH.load(core::sync::atomic::Ordering::Relaxed) != 0
}

#[cfg(all(target_os = "none", target_arch = "x86_64"))]
pub use interrupts::irq_lock_held_on_this_cpu;

#[cfg(all(target_os = "none", target_arch = "x86_64"))]
pub use interrupts::set_gs_depth_ready;

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

            // ── Per-CPU IrqLock depth in TSS.rsp2 (#862/#523) ──────
            //
            // Every CPU in this kernel sets its kernel GS base to its
            // own TSS (BSP: trapframe::init / replace_bsp_tss_with_iopb;
            // APs: init_ap_gdt_idt). TSS.rsp2 (gs:0x14) is unused —
            // hardware only reads it for ring-2 far calls, and
            // trapframe only uses rsp0 (gs:4) and rsp1 (gs:12) — so it
            // is a free per-CPU slot for the IrqLock critical-section
            // depth.
            //
            // Per-CPU (rather than a global counter) matters for two
            // reasons:
            //  1. Correctness of the yield decision: only a lock held
            //     by THIS CPU prevents THIS CPU from yielding.
            //  2. Containment: a panicked CPU spins forever in the
            //     panic handler and never releases its guards — a
            //     global counter would stay nonzero forever and turn
            //     every later gate-context contention into a panic
            //     (the observed secondary-panic cascade).
            //
            // Before the GS base is set up (BSP early boot; APs take
            // no locks before init_ap_gdt_idt), a global flag falls
            // back to the shared counter — safe because only the BSP
            // runs then.

            /// Set once every online CPU's GS base points at its TSS.
            static GS_DEPTH_READY: core::sync::atomic::AtomicBool =
                core::sync::atomic::AtomicBool::new(false);

            /// Enable the per-CPU depth slot. Call after installing
            /// this CPU's TSS as the kernel GS base. See
            /// `replace_bsp_tss_with_iopb` / `init_ap_gdt_idt`.
            pub fn set_gs_depth_ready() {
                GS_DEPTH_READY.store(true, core::sync::atomic::Ordering::Release);
            }

            #[inline]
            pub(crate) fn depth_add(delta: i64) {
                if GS_DEPTH_READY.load(core::sync::atomic::Ordering::Relaxed) {
                    if delta >= 0 {
                        unsafe {
                            core::arch::asm!(
                                "lock inc qword ptr gs:[0x14]",
                                options(nostack)
                            );
                        }
                    } else {
                        unsafe {
                            core::arch::asm!(
                                "lock dec qword ptr gs:[0x14]",
                                options(nostack)
                            );
                        }
                    }
                } else if delta >= 0 {
                    super::IRQ_LOCK_DEPTH
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                } else {
                    super::IRQ_LOCK_DEPTH
                        .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
                }
            }

            /// #523 diagnostics: record the call site of the push that
            /// takes this CPU's depth 0→1 (TSS.ist1, gs:0x24 — free: no
            /// IDT entry uses IST). Read at a stuck-depth state to find
            /// the leaking acquisition.
            #[inline]
            pub(crate) fn depth_debug_record(caller: usize) {
                if GS_DEPTH_READY.load(core::sync::atomic::Ordering::Relaxed) {
                    unsafe {
                        let d: u64;
                        core::arch::asm!(
                            "mov {d}, qword ptr gs:[0x14]",
                            d = out(reg) d,
                            options(nostack, preserves_flags)
                        );
                        if d == 0 {
                            core::arch::asm!(
                                "mov qword ptr gs:[0x24], {c}",
                                c = in(reg) caller,
                                options(nostack, preserves_flags)
                            );
                        }
                    }
                }
            }

            /// Whether this CPU currently holds an IrqLock critical
            /// section.
            #[inline]
            pub fn irq_lock_held_on_this_cpu() -> bool {
                if GS_DEPTH_READY.load(core::sync::atomic::Ordering::Relaxed) {
                    let depth: u64;
                    unsafe {
                        core::arch::asm!(
                            "mov {depth}, qword ptr gs:[0x14]",
                            depth = out(reg) depth,
                            options(nostack, preserves_flags)
                        );
                    }
                    depth != 0
                } else {
                    super::IRQ_LOCK_DEPTH.load(core::sync::atomic::Ordering::Relaxed) != 0
                }
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
    } else if #[cfg(not(target_os = "none"))] {
        // Host builds (libos tests, clippy, docs): a host process
        // cannot control its interrupts. Report "already disabled"
        // so push_off/pop_off become inert no-ops that satisfy the
        // LIFO debug assertion (pop_off's assert requires
        // !was_enabled || !intr_get(); with intr_get() == false
        // every guard sees was_enabled == false and pops nothing).
        mod interrupts {
            pub(crate) fn intr_on() {}
            pub(crate) fn intr_off() {}
            pub(crate) fn intr_get() -> bool {
                false
            }
        }
    } else {
        // Unsupported bare-metal architecture — fail at compile
        // time (#538): a runtime unimplemented!() here would only
        // crash on first lock acquisition, after boot.
        compile_error!(
            "unsupported bare-metal target for zCore's lock crate: \
             implement intr_on/intr_off/intr_get (see \
             zCore/lock/src/interrupt.rs) for this architecture"
        );
    }
}

use interrupts::*;

pub(crate) use interrupts::{intr_get, intr_on};

/// Disable interrupts and return whether they were previously enabled.
///
/// Each lock guard stores this return value and passes it to `pop_off`
/// on drop. Nesting is handled naturally: inner locks see
/// `was_enabled = false` (already off) and their `pop_off` is a no-op.
///
/// Guards must be dropped in LIFO order. Dropping an outer guard first
/// would re-enable interrupts while an inner lock is still held.
#[inline(always)]
#[track_caller]
pub(crate) fn push_off() -> bool {
    let was_enabled = intr_get();
    intr_off();
    #[cfg(all(target_os = "none", target_arch = "x86_64"))]
    let caller = core::panic::Location::caller() as *const _ as usize;
    #[cfg(all(target_os = "none", target_arch = "x86_64"))]
    {
        crate::interrupt::depth_debug_record(caller);
    }
    // #862 fix support: count live interrupt-disabling guards. The
    // KernelMutex contended path uses this to distinguish "IF=0
    // because an IrqLock critical section is held" (must not yield —
    // that would abandon the section) from "IF=0 because the hardware
    // interrupt gate cleared it on trap entry" (task code running
    // inside a trap frame — yielding is the standard preemption
    // pattern). A single global counter is sufficient for the safety
    // question "is any IrqLock held anywhere": it only reaches 0 when
    // no CPU holds one.
    #[cfg(all(target_os = "none", target_arch = "x86_64"))]
    interrupts::depth_add(1);
    #[cfg(not(all(target_os = "none", target_arch = "x86_64")))]
    IRQ_LOCK_DEPTH.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
    #[cfg(all(target_os = "none", target_arch = "x86_64"))]
    interrupts::depth_add(-1);
    #[cfg(not(all(target_os = "none", target_arch = "x86_64")))]
    IRQ_LOCK_DEPTH.fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
    if was_enabled {
        intr_on();
    }
}

//! aarch64 macOS fncall implementation.
//!
//! The trapframe crate's fncall module doesn't support aarch64 macOS.
//! This module provides equivalent functionality adapted for Darwin's
//! aarch64 thread-local storage layout.
//!
//! On Darwin aarch64 (Apple Silicon):
//! - `tpidrro_el0` (read-only) points to the pthread TSD array
//! - `tpidr_el0` is a small integer (thread slot index), NOT a pointer
//! - TSD slots are 8 bytes each, accessed via `[tpidrro_el0 + slot*8]`
//! - We use TSD[6] (offset 48) to store the UserContext pointer
//! - We use TSD[7] (offset 56) to store the kernel stack pointer
//!
//! The user thread pointer for musl is allocated from a separate
//! memory region and stored in context.tpidr. We use `tpidr_el0`
//! (writable) to pass it to user code, since musl reads `tpidr_el0`
//! for its thread pointer on aarch64.

use core::arch::global_asm;
use trapframe::UserContext;

extern "C" {
    pub fn syscall_fn_entry();
    fn syscall_fn_return(regs: &mut UserContext);
}

/// Extension trait to add run_fncall on aarch64 macOS.
pub trait UserContextFnCall {
    fn run_fncall_macos(&mut self);
}

impl UserContextFnCall for UserContext {
    fn run_fncall_macos(&mut self) {
        unsafe {
            syscall_fn_return(self);
        }
    }
}

// Darwin aarch64 fncall assembly.
//
// On Darwin arm64:
// - tpidrro_el0 = TSD base pointer (read-only from EL0, always valid)
// - tpidr_el0   = small integer, writable, we repurpose for user TP
// - TSD[6]  (tpidrro_el0 + 48) = context pointer (scratch slot)
// - TSD[7]  (tpidrro_el0 + 56) = kernel stack pointer (scratch slot)
//
// syscall_fn_return: kernel -> user
//   Save kernel state, load user registers from UserContext, ret to user.
//
// syscall_fn_entry: user -> kernel (called via BL from user's SVC handler)
//   Save user registers into UserContext, restore kernel state, ret to kernel.
//
// User code (musl static binary) issues SVC #0, which on bare metal would
// trap to EL1. In libos mode, we install a Mach exception handler (or
// signal handler) that vectors to syscall_fn_entry. However, the current
// design uses a function-call convention where user code BLs to
// syscall_fn_entry directly (the entry address is patched into the binary
// via the "rcore_syscall_entry" symbol). For static musl busybox, there's
// no such symbol, so we need the signal-based approach.
global_asm!(
    r#"
.global _syscall_fn_entry
.global _syscall_fn_return
.set syscall_fn_entry, _syscall_fn_entry
.set syscall_fn_return, _syscall_fn_return

_syscall_fn_entry:
    // Entered from user code. tpidrro_el0 = TSD base.
    // TSD[6] = context pointer, TSD[7] = kernel sp.
    // Save x0, x30 on user stack for scratch use.
    stp     x0, x30, [sp, #-16]

    // Load context pointer from TSD[6]
    mrs     x0, tpidrro_el0        // x0 = TSD base (read-only reg)
    ldr     x0, [x0, #48]          // x0 = context pointer (TSD[6])

    // Save user sp
    mov     x30, sp
    str     x30, [x0, #4 * 8]     // context.sp = user sp (before push)
    add     sp, x0, #38 * 8       // sp = top of UserContext struct

    // Recover x0, x30 from user stack
    ldp     x0, x30, [x30]

    // Save general registers (x30..x0) into UserContext
    stp     x30, x0, [sp, #-16]!   // x30 (lr), x0
    str     x29, [sp, #-16]!
    stp     x27, x28, [sp, #-16]!
    stp     x25, x26, [sp, #-16]!
    stp     x23, x24, [sp, #-16]!
    stp     x21, x22, [sp, #-16]!
    stp     x19, x20, [sp, #-16]!
    stp     x17, x18, [sp, #-16]!
    stp     x15, x16, [sp, #-16]!
    stp     x13, x14, [sp, #-16]!
    stp     x11, x12, [sp, #-16]!
    stp     x9, x10, [sp, #-16]!
    stp     x7, x8, [sp, #-16]!
    stp     x5, x6, [sp, #-16]!
    stp     x3, x4, [sp, #-16]!
    stp     x1, x2, [sp, #-16]!

    // Save tpidr_el0 (user thread pointer) into context.tpidr
    mrs     x1, tpidr_el0
    str     x1, [sp, #-8]
    add     sp, sp, #-16

    // Save elr (return address = x30 saved above) into context.elr
    ldr     x1, [sp, #32*8]
    str     x1, [sp, #-16]!

    // Restore kernel sp from TSD[7]
    mrs     x1, tpidrro_el0
    ldr     x1, [x1, #56]          // x1 = kernel sp (TSD[7])
    mov     sp, x1

    // Restore kernel tpidr_el0
    ldr     x1, [sp], #16
    msr     tpidr_el0, x1

    // Restore callee-saved registers
    ldp     x19, x20, [sp], #16
    ldp     x21, x22, [sp], #16
    ldp     x23, x24, [sp], #16
    ldp     x25, x26, [sp], #16
    ldp     x27, x28, [sp], #16
    ldp     x29, x30, [sp], #16

    ret

_syscall_fn_return:
    // Entered from kernel. x0 = &mut UserContext.
    // Save callee-saved registers on kernel stack.
    stp     x29, x30, [sp, #-16]!
    stp     x27, x28, [sp, #-16]!
    stp     x25, x26, [sp, #-16]!
    stp     x23, x24, [sp, #-16]!
    stp     x21, x22, [sp, #-16]!
    stp     x19, x20, [sp, #-16]!

    // Save kernel tpidr_el0 on kernel stack
    mrs     x8, tpidr_el0
    str     x8, [sp, #-16]!

    // Save kernel sp to TSD[7] (via tpidrro_el0)
    mrs     x9, tpidrro_el0        // x9 = TSD base
    mov     x10, sp
    str     x10, [x9, #56]         // TSD[7] = kernel sp

    // Store context pointer in TSD[6]
    str     x0, [x9, #48]          // TSD[6] = context pointer

    // Setup user thread pointer
    ldr     x10, [x0, #5*8]        // x10 = context.tpidr (user tp)
    cbnz    x10, 1f                 // if set, use it
    // First entry: allocate user TP area at TSD[30] (offset 240)
    add     x10, x9, #240          // x10 = TSD base + 240
    str     x10, [x10]             // user_tp:0 = self (musl convention)
1:  msr     tpidr_el0, x10          // set user thread pointer

    // Store context pointer at user_tp + 48 (musl canary2 slot)
    str     x0, [x10, #48]

    // Load elr (entry point) and user sp
    ldr     x30, [x0, #2*8]       // x30 = elr
    ldr     x8, [x0, #4*8]        // x8 = user sp
    mov     sp, x8

    // Load general registers from UserContext
    add     x0, x0, #6*8
    ldp     x1, x2, [x0], #16
    ldp     x3, x4, [x0], #16
    ldp     x5, x6, [x0], #16
    ldp     x7, x8, [x0], #16
    ldp     x9, x10, [x0], #16
    ldp     x11, x12, [x0], #16
    ldp     x13, x14, [x0], #16
    ldp     x15, x16, [x0], #16
    ldp     x17, x18, [x0], #16
    ldp     x19, x20, [x0], #16
    ldp     x21, x22, [x0], #16
    ldp     x23, x24, [x0], #16
    ldp     x25, x26, [x0], #16
    ldp     x27, x28, [x0], #16
    ldr     x29, [x0], #16
    ldr     x0, [x0, #8]
    ret                             // jump to user entry (x30)
"#
);

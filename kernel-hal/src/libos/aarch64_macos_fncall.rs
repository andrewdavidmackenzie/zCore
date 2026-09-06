//! aarch64 macOS fncall implementation.
//!
//! The trapframe crate's fncall module doesn't support aarch64 macOS.
//! This module provides equivalent functionality using the same
//! mechanism as the aarch64 Linux fncall, adapted for Darwin's
//! pthread TSD layout.
//!
//! On Darwin aarch64:
//! - `tpidr_el0` points to the pthread TSD (Thread Specific Data) array
//! - TSD slots are 8 bytes each
//! - We use TSD[6] (offset 48) for kernel stack pointer
//! - We use TSD[30] (offset 240) for init user TP
//! - User programs (musl) store context at TP+48 (pthread.canary2)
//!
//! This matches the x86_64 macOS fncall's TSD slot usage.

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
// User: (musl)
// - tp:0  (pthread.self)       = user tp
// - tp:48 (pthread.canary2)    = user context pointer
//
// Kernel: (darwin pthread)
// - tpidr_el0 points to TSD array
// - TSD[6]  (offset 48)  = kernel stack
// - TSD[30] (offset 240) = init user tp
//
// Note: On Darwin, tpidr_el0 points directly to the TSD array,
// unlike glibc where it points to the pthread struct.
global_asm!(
    r#"
.global _syscall_fn_entry
.global _syscall_fn_return
.set syscall_fn_entry, _syscall_fn_entry
.set syscall_fn_return, _syscall_fn_return

_syscall_fn_entry:
    // save 2 registers for scratch
    stp     x0, x30, [sp, #-16]    // save x0, x30 at user stack

    // switch to kernel sp
    mrs     x0, tpidr_el0          // x0 = user tp (TSD base)
    ldr     x0, [x0, #48]         // x0 = user context (TSD[6])
    mov     x30, sp                // x30 = user stack
    str     x30, [x0, #4 * 8]     // save user stack to context.sp
    add     sp, x0, #38 * 8       // sp = top of user context

    // recover x0, x30
    ldp     x0, x30, [x30, #-16]

    // save general registers
    stp     x30, x0, [sp, #-16]!
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

    // skip sp and save tpidr
    mrs     x1, tpidr_el0
    str     x1, [sp, #-8]
    add     sp, sp, #-16

    // skip spsr and save elr(lr)
    ldr     x1, [sp, #32*8]
    str     x1, [sp, #-16]!

    // skip trap num and read kernel sp
    ldr     x1, [sp, #-8]
    mov     sp, x1

    // load kernel tp (restore original tpidr_el0)
    ldr     x1, [sp], #16
    msr     tpidr_el0, x1

    // load callee-saved registers
    ldp     x19, x20, [sp], #16
    ldp     x21, x22, [sp], #16
    ldp     x23, x24, [sp], #16
    ldp     x25, x26, [sp], #16
    ldp     x27, x28, [sp], #16
    ldp     x29, x30, [sp], #16

    ret

_syscall_fn_return:
    // save callee-saved registers
    stp     x29, x30, [sp, #-16]!
    stp     x27, x28, [sp, #-16]!
    stp     x25, x26, [sp, #-16]!
    stp     x23, x24, [sp, #-16]!
    stp     x21, x22, [sp, #-16]!
    stp     x19, x20, [sp, #-16]!

    // save kernel tp
    mrs     x8, tpidr_el0          // x8 = kernel tp
    str     x8, [sp, #-16]!

    // save kernel sp to UserContext
    mov     x9, sp
    str     x9, [x0, #8]          // context.kernel_sp = sp

    // setup user tp
    ldr     x9, [x0, #5*8]        // x9 = user tp from context
    cbnz    x9, 1f                 // if not 0, use it
    // init user tp: use TSD[30] area
    add     x9, x8, #240          // x9 = kernel_tp + 240 (TSD[30])
    mov     x10, x9
    str     x10, [x9]             // user_tp:0 = self
1:  msr     tpidr_el0, x9          // set user tp
    str     x0, [x9, #48]         // user_tp:48 = context pointer

    // pop elr, sp
    ldr     x30, [x0, #2*8]       // x30 = elr (entry point)
    ldr     x8, [x0, #4*8]        // x8 = user sp
    mov     sp, x8

    // pop general registers
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
    ret
"#
);

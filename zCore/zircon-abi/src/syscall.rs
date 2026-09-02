//! Inline syscall wrappers for Zircon userspace programs.
//!
//! These use hardware trap instructions to enter the kernel:
//! - `svc #0` on aarch64 (syscall number in x16)
//! - `syscall` on x86_64 (syscall number in rax)
//! - `ecall` on riscv64 (syscall number in a7)
//!
//! # Safety
//! All functions are unsafe because they perform raw syscalls with
//! unchecked arguments.

use crate::errors::ZxStatus;

/// Raw syscall with 0 arguments.
#[inline(always)]
pub unsafe fn syscall0(num: u32) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 1 argument.
#[inline(always)]
pub unsafe fn syscall1(num: u32, a0: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 2 arguments.
#[inline(always)]
pub unsafe fn syscall2(num: u32, a0: u64, a1: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 3 arguments.
#[inline(always)]
pub unsafe fn syscall3(num: u32, a0: u64, a1: u64, a2: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        in("a2") a2,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 4 arguments.
///
/// Note: on x86_64, the 4th argument uses `r10` (not `rcx`) because the
/// `syscall` instruction clobbers `rcx` (stores RIP). This matches the
/// real Zircon/Linux syscall ABI. These wrappers are for bare-metal use
/// only (behind the `userspace` feature), not for libos mode.
#[inline(always)]
pub unsafe fn syscall4(num: u32, a0: u64, a1: u64, a2: u64, a3: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        in("r10") a3,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        in("a2") a2,
        in("a3") a3,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 5 arguments.
#[inline(always)]
pub unsafe fn syscall5(num: u32, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3, in("x4") a4,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4,
        lateout("rax") ret,
        out("rcx") _, out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3, in("a4") a4,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 6 arguments.
#[inline(always)]
pub unsafe fn syscall6(num: u32, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3, in("x4") a4, in("x5") a5,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4, in("r9") a5,
        lateout("rax") ret,
        out("rcx") _, out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3, in("a4") a4, in("a5") a5,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 7 arguments.
///
/// On x86_64 the 7th argument is passed on the stack (the `syscall`
/// instruction only provides 6 register arguments). On aarch64 and
/// riscv64, registers x6/a6 are used.
#[inline(always)]
pub unsafe fn syscall7(
    num: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3,
        in("x4") a4, in("x5") a5, in("x6") a6,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    {
        // x86_64 syscall ABI only has 6 register args. The 7th is on the stack.
        // Push it, syscall, pop it.
        core::arch::asm!(
            "push {arg6}",
            "syscall",
            "pop {arg6}",
            arg6 = in(reg) a6,
            in("eax") num,
            in("rdi") a0, in("rsi") a1, in("rdx") a2,
            in("r10") a3, in("r8") a4, in("r9") a5,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
        );
    }
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3,
        in("a4") a4, in("a5") a5, in("a6") a6,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 8 arguments.
///
/// On x86_64 args 7 and 8 are passed on the stack.
/// On aarch64 and riscv64, registers x7/a7 are used (note: a7 is also
/// the syscall number register on riscv64, loaded before the args).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub unsafe fn syscall8(
    num: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
    a7: u64,
) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3,
        in("x4") a4, in("x5") a5, in("x6") a6, in("x7") a7,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    {
        // x86_64: args 7 and 8 on the stack
        core::arch::asm!(
            "push {arg7}",
            "push {arg6}",
            "syscall",
            "pop {arg6}",
            "pop {arg7}",
            arg6 = in(reg) a6,
            arg7 = in(reg) a7,
            in("eax") num,
            in("rdi") a0, in("rsi") a1, in("rdx") a2,
            in("r10") a3, in("r8") a4, in("r9") a5,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
        );
    }
    #[cfg(target_arch = "riscv64")]
    {
        // riscv64: a7 is both the syscall number and the 8th argument register.
        // Load the syscall number first, then the 8th arg overwrites a7 --
        // but ecall reads the syscall number from a7 at the point of the trap.
        // To avoid the conflict, we store the syscall number in a7 last.
        // Actually, on riscv64 the Zircon ABI uses a7 for syscall number
        // and only supports 7 register arguments (a0-a6).
        // 8-arg syscalls on riscv64 would need a stack-based convention.
        // For now, this is a compile error -- no 8-arg riscv64 syscalls needed yet.
        compile_error!("8-argument syscalls not yet supported on riscv64");
    }
    ret as ZxStatus
}

// --- Typed wrappers for common syscalls ---

/// Write a debug message to the kernel log.
///
/// # Safety
/// `buf` must point to `len` valid bytes.
pub unsafe fn zx_debug_write(buf: *const u8, len: usize) -> ZxStatus {
    syscall2(crate::consts::SYS_DEBUG_WRITE, buf as u64, len as u64)
}

/// Exit the current process.
pub unsafe fn zx_process_exit(retcode: i64) -> ! {
    syscall1(crate::consts::SYS_PROCESS_EXIT, retcode as u64);
    // process_exit never returns, but the compiler needs this
    core::hint::unreachable_unchecked()
}

/// Exit the current thread.
pub unsafe fn zx_thread_exit() -> ! {
    syscall0(crate::consts::SYS_THREAD_EXIT);
    core::hint::unreachable_unchecked()
}

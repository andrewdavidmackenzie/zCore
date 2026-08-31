//! Inline syscall wrappers for Zircon userspace programs.
//!
//! These use `svc #0` (aarch64) or `syscall` (x86_64) to trap into the kernel.
//! The syscall number is passed in x16 (aarch64) or rax (x86_64).
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
    ret as ZxStatus
}

/// Raw syscall with 4 arguments.
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

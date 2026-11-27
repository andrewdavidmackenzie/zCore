//! petal AVX/YMM state preservation test (#907).
//!
//! Two threads in one process hammer syscalls with distinct YMM
//! patterns loaded in ymm0. Every syscall is a full user→kernel→user
//! transition through `enter_uspace`, which saves and restores
//! extended register state. Without XSAVE, FXSAVE loses the YMM
//! upper halves: each thread's pattern gets corrupted by the other's
//! (or reset), and the verify fails.
//!
//! The kernel itself is compiled without SSE/AVX, so nothing but the
//! context switch can touch ymm0 — a verify failure is a context
//! switch bug.
//!
//! NOTE on negative controls: under QEMU TCG the FXSAVE fallback
//! also passes this test — TCG's FXSAVE emulation preserves the YMM
//! high halves, unlike real hardware. On real hardware (or KVM),
//! FXSAVE loses them and this test fails. Under TCG the test
//! primarily validates that the XSAVE path is armed, boots, and
//! round-trips every user-mode transition without faulting; the
//! preservation property is validated by construction (XSAVE covers
//! the AVX state component by spec).

#![no_std]
#![no_main]

extern crate petal;

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(target_arch = "x86_64")]
use zircon_abi::consts::ZX_THREAD_TERMINATED;
#[cfg(target_arch = "x86_64")]
use zircon_abi::processargs::{PA_PROC_SELF, PA_VMAR_ROOT};
#[cfg(target_arch = "x86_64")]
use zx::sys::*;

#[cfg(target_arch = "x86_64")]
const ZX_TIME_INFINITE: i64 = i64::MAX;

#[cfg(target_arch = "x86_64")]
static STOP: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "x86_64")]
static ERRORS: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "x86_64")]
#[cfg(target_arch = "x86_64")]
fn pattern_for(id: u8) -> [u8; 32] {
    [0xA0 | id; 32]
}

/// Enough threads that, on an 8-CPU guest, pigeonhole forces at
/// least three threads onto one CPU — the cross-thread YMM
/// corruption scenario. (zCore does not enforce affinity masks, so
/// pinning cannot force sharing; count does.)
#[cfg(target_arch = "x86_64")]
const NUM_THREADS: usize = 16;

#[cfg(target_arch = "x86_64")]
const ITERATIONS: u32 = 100_000;
#[cfg(target_arch = "x86_64")]
const STACK_LEN: usize = 64 * 1024;

// The AVX instructions are raw VEX encodings (.byte), not mnemonics:
// the petal target is soft-float and LLVM's asm parser rejects AVX
// mnemonics (and target_feature is unsupported on it), but the CPU
// decodes VEX fine — the instructions execute identically.
//
//   C5 FE 6F 00 = vmovdqu ymm0, [rax]
//   C5 FE 7F 00 = vmovdqu [rax], ymm0
// (VEX.256.66.0F.W0; ModRM 00 000 000: ymm0 ↔ [rax])
#[cfg(target_arch = "x86_64")]
#[inline(never)]
unsafe fn ymm0_load(pattern: &[u8; 32]) {
    core::arch::asm!(
        ".byte 0xC5, 0xFE, 0x6F, 0x00",
        in("rax") pattern.as_ptr(),
        options(nostack),
    );
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
unsafe fn ymm0_verify(pattern: &[u8; 32]) -> bool {
    let mut buf = [0u8; 32];
    core::arch::asm!(
        ".byte 0xC5, 0xFE, 0x7F, 0x00",
        in("rax") buf.as_mut_ptr(),
        options(nostack),
    );
    buf == *pattern
}

/// Load the pattern once, then across many syscall round-trips verify
/// it survives. On a mismatch, record it and re-arm so each loss is
/// counted distinctly.
#[cfg(target_arch = "x86_64")]
unsafe fn avx_loop(pattern: &[u8; 32]) {
    ymm0_load(pattern);
    for _ in 0..ITERATIONS {
        // A full user→kernel→user transition.
        let _ = zx_clock_get_monotonic_via_kernel();
        if !ymm0_verify(pattern) {
            ERRORS.fetch_add(1, Ordering::Relaxed);
            ymm0_load(pattern);
        }
        if STOP.load(Ordering::Relaxed) {
            return;
        }
    }
}

/// Entry point for the worker threads. `arg1` carries the thread's
/// index (its distinct YMM pattern id).
#[cfg(target_arch = "x86_64")]
#[no_mangle]
extern "C" fn avx_thread_entry(arg1: usize, _arg2: usize) {
    let pattern = pattern_for(arg1 as u8);
    unsafe {
        avx_loop(&pattern);
    }
    unsafe { zx_thread_exit() }
}

/// Whether this CPU advertises AVX (CPUID leaf 1, ECX bit 28).
/// QEMU 6.2's TCG does not provide AVX even under `-cpu max`, so the
/// test must skip rather than execute VEX bytes into a #UD
/// (CodeRabbit review on #911).
#[cfg(target_arch = "x86_64")]
fn cpu_has_avx() -> bool {
    let ecx: u32;
    unsafe {
        // rbx is reserved by LLVM — save/restore it around cpuid.
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "pop rbx",
            lateout("ecx") ecx,
            out("eax") _,
            out("edx") _,
        );
    }
    ecx & (1 << 28) != 0
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
pub fn main() {
    zx::debug_write(b"avx_test: starting\n");
    if !cpu_has_avx() {
        zx::debug_write(b"avx_test: SKIPPED (no AVX on this CPU) PASS\n");
        zx::Process::exit(0);
    }

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: avx_test - no bootstrap\n");
        zx::Process::exit(1);
    });
    let proc_h = bootstrap.find(PA_PROC_SELF);
    let vmar_h = bootstrap.find(PA_VMAR_ROOT);
    if proc_h == 0 || vmar_h == 0 {
        zx::debug_write(b"FAIL: avx_test - missing bootstrap handles\n");
        zx::Process::exit(1);
    }

    // Map one big stack region and carve per-thread stacks from it.
    let total_stack: usize = STACK_LEN * NUM_THREADS;
    let mut stack_vmo: HandleValue = 0;
    let s = unsafe { zx_vmo_create(total_stack as u64, 0, &mut stack_vmo) };
    if s != 0 {
        zx::debug_write(b"FAIL: avx_test - vmo create\n");
        zx::Process::exit(1);
    }
    let mut stack_addr: usize = 0;
    // ZX_VM_PERM_READ | ZX_VM_PERM_WRITE = 0x3
    let s = unsafe { zx_vmar_map(vmar_h, 0x3, 0, stack_vmo, 0, total_stack, &mut stack_addr) };
    if s != 0 {
        zx::debug_write(b"FAIL: avx_test - vmar map\n");
        zx::Process::exit(1);
    }

    // Spawn the worker threads.
    let entry = avx_thread_entry as *const () as usize;
    let mut thread_handles: [HandleValue; NUM_THREADS] = [0; NUM_THREADS];
    for i in 0..NUM_THREADS {
        let mut thread_h: HandleValue = 0;
        let s = unsafe { zx_thread_create(proc_h, b"avx-w".as_ptr(), 4, 0, &mut thread_h) };
        if s != 0 {
            zx::debug_write(b"FAIL: avx_test - thread create\n");
            zx::Process::exit(1);
        }
        // SysV ABI: at the thread entry, rsp must be 16-byte aligned
        // minus 8 (the layout the entry expects as if called). The
        // mapped region's top is 4096-aligned; subtract 8 to get the
        // correct entry alignment (CodeRabbit review on #911).
        let stack_top = stack_addr + STACK_LEN * (i + 1) - 8;
        let s = unsafe { zx_thread_start(thread_h, entry, stack_top, i, 0) };
        if s != 0 {
            zx::debug_write(b"FAIL: avx_test - thread start\n");
            zx::Process::exit(1);
        }
        thread_handles[i] = thread_h;
        let _ = &thread_handles; // clippy: index used, keep the array
    }

    // The main thread's loop (pattern id = last index), concurrent
    // with the workers.
    let main_pattern = pattern_for(NUM_THREADS as u8);
    unsafe { avx_loop(&main_pattern) };
    STOP.store(true, Ordering::Release);

    // Wait for all workers to terminate.
    for thread_h in &thread_handles {
        let mut observed: u32 = 0;
        let _ = unsafe {
            zx_object_wait_one(
                *thread_h,
                ZX_THREAD_TERMINATED,
                ZX_TIME_INFINITE,
                &mut observed,
            )
        };
    }

    let errors = ERRORS.load(Ordering::Relaxed);
    if errors == 0 {
        zx::debug_write(b"avx_test: ymm_preserved PASS\n");
        zx::debug_write(b"avx_test: all tests passed\n");
        zx::Process::exit(0);
    } else {
        zx::debug_write(b"FAIL: avx_test - ymm corrupted across context switches: ");
        // Print the count in decimal.
        let mut digits = [b'0'; 10];
        let mut n = errors as usize;
        let mut i = digits.len();
        loop {
            i -= 1;
            digits[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        zx::debug_write(&digits[i..]);
        zx::debug_write(b"\n");
        zx::Process::exit(1);
    }
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
pub fn main() {
    // AVX is x86_64-only; report pass so the shared runner list
    // stays architecture-independent.
    unsafe {
        zx::sys::zx_debug_write(b"avx_test: SKIPPED (x86_64 only) PASS\n".as_ptr(), 34);
        zx::sys::zx_process_exit(0);
    }
}

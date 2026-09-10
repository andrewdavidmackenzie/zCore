//! aarch64 macOS fncall implementation.
//!
//! On Darwin aarch64 (Apple Silicon), user code executes `svc #0` for
//! syscalls. macOS delivers this as SIGSYS. We install a signal handler
//! that saves the user register state into the UserContext, then longjmps
//! back to the kernel.
//!
//! Flow:
//!   kernel: run_fncall_macos()
//!     -> setjmp to save kernel state
//!     -> load user registers, ret to user code
//!   user: executes, eventually does svc #0
//!   macOS: delivers SIGSYS
//!     -> sigsys_handler reads user regs from mcontext
//!     -> copies them into UserContext
//!     -> longjmp back to kernel
//!
//! Thread-local storage on Darwin aarch64:
//! - `tpidrro_el0` = pthread TSD base (read-only, valid pointer)
//! - `tpidr_el0` = small integer (thread slot index), NOT a pointer
//! - We store the UserContext pointer in TSD[6] (offset 48)

use core::arch::global_asm;
use trapframe::UserContext;

extern "C" {
    /// Dummy entry point (not used -- SIGSYS handler replaces this).
    pub fn syscall_fn_entry();
}

/// Install signal handlers for intercepting `svc #0` (SIGSYS) and
/// catching crashes in user code (SIGSEGV, SIGBUS).
/// Must be called once during initialization.
pub fn install_sigsys_handler() {
    // Ensure SIGSYS is not blocked on this thread (the async runtime
    // or another library may have blocked it).
    unsafe {
        let mut unblock: nix::libc::sigset_t = core::mem::zeroed();
        nix::libc::sigemptyset(&mut unblock);
        nix::libc::sigaddset(&mut unblock, nix::libc::SIGSYS);
        nix::libc::sigaddset(&mut unblock, nix::libc::SIGSEGV);
        nix::libc::sigaddset(&mut unblock, nix::libc::SIGBUS);
        nix::libc::pthread_sigmask(nix::libc::SIG_UNBLOCK, &unblock, core::ptr::null_mut());
    }
    unsafe {
        // Use SA_ONSTACK so macOS delivers signals on the alternate signal
        // stack instead of the current SP. This is critical because when
        // user code runs, SP points to MAP_ANON user memory that macOS
        // cannot use for signal frame setup.
        let mut sa: nix::libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = sigsys_handler as *const () as usize;
        sa.sa_flags = nix::libc::SA_SIGINFO | nix::libc::SA_ONSTACK;
        nix::libc::sigemptyset(&mut sa.sa_mask);
        let ret = nix::libc::sigaction(nix::libc::SIGSYS, &sa, core::ptr::null_mut());
        if ret != 0 {
            panic!("Failed to install SIGSYS handler");
        }

        // Also intercept SIGSEGV and SIGBUS from user code
        let mut sa2: nix::libc::sigaction = core::mem::zeroed();
        sa2.sa_sigaction = user_fault_handler as *const () as usize;
        sa2.sa_flags = nix::libc::SA_SIGINFO | nix::libc::SA_ONSTACK;
        nix::libc::sigemptyset(&mut sa2.sa_mask);
        nix::libc::sigaction(nix::libc::SIGSEGV, &sa2, core::ptr::null_mut());
        nix::libc::sigaction(nix::libc::SIGBUS, &sa2, core::ptr::null_mut());
    }
    info!("Installed SIGSYS/SIGSEGV/SIGBUS handlers for aarch64 macOS libos");
}

// Per-thread state for the setjmp/longjmp kernel return.
// Stored in thread-local storage since each async-std worker thread
// needs its own jump buffer.
std::thread_local! {
    static KERNEL_JMP_BUF: std::cell::UnsafeCell<[u64; 32]> =
        std::cell::UnsafeCell::new([0u64; 32]);
    static CURRENT_CTX: std::cell::Cell<*mut UserContext> =
        std::cell::Cell::new(core::ptr::null_mut());
    /// Per-thread copy of UserContext for the noreturn jump to user code.
    /// We copy the context here because the compiler may free the caller's
    /// stack frame before a noreturn callee reads a pointer argument.
    /// Thread-local storage is not affected by stack frame deallocation.
    static JUMP_CTX: std::cell::UnsafeCell<UserContext> =
        std::cell::UnsafeCell::new(unsafe { core::mem::zeroed() });
}

extern "C" {
    fn _aarch64_setjmp(buf: *mut u64) -> i32;
    fn _aarch64_longjmp(buf: *mut u64, val: i32) -> !;
    fn _aarch64_jump_to_user(ctx: *const UserContext) -> !;
    #[allow(dead_code)]
    fn _aarch64_sigsys_trampoline();
}

/// Trampoline helper -- called by `__aarch64_sigsys_trampoline` assembly.
/// The trampoline approach is no longer the primary path (longjmp is called
/// directly from the SIGSYS handler), but this must exist because the
/// assembly label references it.
#[no_mangle]
unsafe extern "C" fn _aarch64_do_longjmp() {
    KERNEL_JMP_BUF.with(|buf| {
        _aarch64_longjmp((*buf.get()).as_mut_ptr(), 1);
    });
}

/// Extension trait to add run_fncall on aarch64 macOS.
pub trait UserContextFnCall {
    fn run_fncall_macos(&mut self);
}

/// Size of the alternate signal stack (per thread).
const SIGALTSTACK_SIZE: usize = 64 * 1024; // 64 KiB

std::thread_local! {
    /// Per-thread alternate signal stack memory.
    /// Allocated once per worker thread, freed when the thread exits.
    static SIGALT_STACK: std::cell::RefCell<Option<Vec<u8>>> =
        std::cell::RefCell::new(None);
}

/// Ensure the current thread has an alternate signal stack configured.
/// Must be called on each worker thread before entering user code.
fn ensure_sigaltstack() {
    SIGALT_STACK.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_some() {
            return; // Already set up
        }
        let stack_mem = vec![0u8; SIGALTSTACK_SIZE];
        unsafe {
            let ss = nix::libc::stack_t {
                ss_sp: stack_mem.as_ptr() as *mut _,
                ss_flags: 0,
                ss_size: SIGALTSTACK_SIZE,
            };
            let ret = nix::libc::sigaltstack(&ss, core::ptr::null_mut());
            if ret != 0 {
                panic!("sigaltstack failed: {}", std::io::Error::last_os_error());
            }
        }
        *slot = Some(stack_mem); // Keep alive
    });
}

impl UserContextFnCall for UserContext {
    fn run_fncall_macos(&mut self) {
        // Ensure SIGSYS is unblocked on this worker thread.
        // async-std or other libraries may inherit a blocked mask.
        unsafe {
            let mut unblock: nix::libc::sigset_t = core::mem::zeroed();
            nix::libc::sigemptyset(&mut unblock);
            nix::libc::sigaddset(&mut unblock, nix::libc::SIGSYS);
            nix::libc::sigaddset(&mut unblock, nix::libc::SIGSEGV);
            nix::libc::sigaddset(&mut unblock, nix::libc::SIGBUS);
            nix::libc::pthread_sigmask(nix::libc::SIG_UNBLOCK, &unblock, core::ptr::null_mut());
        }
        // Set up alternate signal stack so macOS can deliver signals
        // even when SP points to user-mapped (MAP_ANON) memory.
        ensure_sigaltstack();
        CURRENT_CTX.with(|c| c.set(self as *mut _));
        KERNEL_JMP_BUF.with(|buf| {
            let buf_ptr = unsafe { (*buf.get()).as_mut_ptr() };
            let ret = unsafe { _aarch64_setjmp(buf_ptr) };
            if ret == 0 {
                // First return from setjmp: jump to user code
                trace!(
                    "jump_to_user: elr={:#x}, sp={:#x}, x0={:#x}",
                    self.elr,
                    self.sp,
                    self.general.x0
                );

                // Read register values from self into locals while
                // the stack frame is valid, then pass as inline asm
                // register inputs.
                //
                // Restored: elr, sp, x0-x5 (all 6 syscall args),
                // x8 (syscall number). This covers all Linux aarch64
                // syscalls which use x0-x5 for arguments, x8 for the
                // syscall number, and return the result in x0.
                //
                // We pin elr and sp to specific scratch registers
                // (x9, x10) to prevent the register allocator from
                // assigning them to registers that the asm block
                // writes to (e.g., x8), which would cause conflicts.
                let elr = self.elr;
                let user_sp = self.sp;
                let x0 = self.general.x0;
                let x1 = self.general.x1;
                let x2 = self.general.x2;
                let x3 = self.general.x3;
                let x4 = self.general.x4;
                let x5 = self.general.x5;
                let x8 = self.general.x8;
                // Pin ALL inputs to explicit registers to prevent
                // the register allocator from creating conflicts.
                unsafe {
                    core::arch::asm!(
                        "mov x0, x11",
                        "mov x1, x12",
                        "mov x2, x13",
                        "mov x3, x14",
                        "mov x4, x15",
                        "mov x5, x17",
                        "mov x8, x20",
                        // Set x16 to -1 (all bits set). macOS uses x16 as
                        // the BSD syscall number. With x16 = 0xffff, XNU
                        // corrupts x0 (sets ENOSYS) and x1 (sets 0).
                        // With x16 = -1, XNU delivers SIGSYS without
                        // corrupting x0 or x1.
                        "movn x16, #0",
                        "mov sp, x10",
                        "br x9",
                        in("x9") elr,
                        in("x10") user_sp,
                        in("x11") x0,
                        in("x12") x1,
                        in("x13") x2,
                        in("x14") x3,
                        in("x15") x4,
                        in("x17") x5,
                        in("x20") x8,
                        options(noreturn),
                    );
                }
            }
            // ret != 0: returned via longjmp from SIGSYS handler.
            // UserContext has been populated by the handler.
            trace!(
                "longjmp return: elr={:#x}, sp={:#x}, x8={:#x}",
                self.elr,
                self.sp,
                self.general.x8
            );
        });
    }
}

/// Fault handler for SIGSEGV/SIGBUS from user code.
/// Uses raw write() instead of log macros (async-signal-safe).
unsafe extern "C" fn user_fault_handler(
    sig: i32,
    info: *mut nix::libc::siginfo_t,
    ctx: *mut nix::libc::c_void,
) {
    let uc = ctx as *const nix::libc::ucontext_t;
    let mc = (*uc).uc_mcontext as *const u8;
    const ES_SIZE: usize = 16;
    let ts = mc.add(ES_SIZE) as *const u64;
    let pc = *ts.add(32) as usize;
    let fault_addr = (*info).si_addr as usize;

    // Use raw write() -- signal-safe, no allocations or locks.
    let mut buf = [0u8; 128];
    let msg = if sig == nix::libc::SIGSEGV {
        "SIGSEGV"
    } else {
        "SIGBUS"
    };
    let n = fmt_hex(&mut buf, msg, pc, fault_addr);
    nix::libc::write(2, buf.as_ptr() as *const _, n);
    std::process::abort();
}

/// Format a crash message into a fixed buffer (no allocation).
fn fmt_hex(buf: &mut [u8; 128], sig: &str, pc: usize, addr: usize) -> usize {
    let mut i = 0;
    for &b in sig.as_bytes() {
        buf[i] = b;
        i += 1;
    }
    for &b in b" pc=0x" {
        buf[i] = b;
        i += 1;
    }
    i += write_hex(&mut buf[i..], pc);
    for &b in b" addr=0x" {
        buf[i] = b;
        i += 1;
    }
    i += write_hex(&mut buf[i..], addr);
    buf[i] = b'\n';
    i + 1
}

fn write_hex(buf: &mut [u8], val: usize) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 16];
    let mut n = 0;
    let mut v = val;
    while v > 0 {
        tmp[n] = b"0123456789abcdef"[(v & 0xf) as usize];
        v >>= 4;
        n += 1;
    }
    for j in 0..n {
        buf[j] = tmp[n - 1 - j];
    }
    n
}

/// SIGSYS signal handler. Called when user code executes `svc #0x80`.
/// Reads user registers from the signal mcontext, populates the
/// UserContext, and longjmps back to the kernel.
unsafe extern "C" fn sigsys_handler(
    _sig: i32,
    _info: *mut nix::libc::siginfo_t,
    ctx: *mut nix::libc::c_void,
) {
    // On macOS aarch64, the ucontext_t contains a pointer to
    // __darwin_mcontext64. We need to find the thread state within it.
    //
    // ucontext_t layout (macOS arm64):
    //   int uc_onstack
    //   sigset_t uc_sigmask
    //   stack_t uc_stack
    //   ucontext_t *uc_link
    //   size_t uc_mcsize
    //   mcontext_t uc_mcontext  <-- pointer to __darwin_mcontext64
    //
    // __darwin_mcontext64 layout:
    //   __darwin_arm_exception_state64 __es  (8 bytes: far, esr, exception)
    //   __darwin_arm_thread_state64 __ss
    //     uint64_t x[29]        // x0-x28
    //     uint64_t fp            // x29
    //     uint64_t lr            // x30
    //     uint64_t sp
    //     uint64_t pc
    //     uint32_t cpsr
    //     uint32_t __pad
    //
    // nix::libc::ucontext_t doesn't expose mcontext on macOS arm64,
    // so we use raw pointer arithmetic.

    // uc_mcontext is at a fixed offset in ucontext_t.
    // On macOS arm64: offset varies but we can use the C struct.
    // Actually, nix::libc defines ucontext_t with uc_mcontext as a pointer.
    let uc = ctx as *const nix::libc::ucontext_t;
    let mc = (*uc).uc_mcontext as *const u8;

    // __darwin_arm_exception_state64 is 24 bytes on arm64:
    //   uint64_t __far (8 bytes)
    //   uint32_t __esr (4 bytes)
    //   uint32_t __exception (4 bytes)
    // Total: 16 bytes, but may have padding.
    // Actually, looking at the Darwin headers:
    //   struct __darwin_arm_exception_state64 {
    //       __uint64_t __far;       // 8 bytes
    //       __uint32_t __esr;       // 4 bytes
    //       __uint32_t __exception; // 4 bytes
    //   };  // total 16 bytes
    const ES_SIZE: usize = 16;

    let ts = mc.add(ES_SIZE) as *const u64;
    // ts points to __darwin_arm_thread_state64:
    //   x[0..29]  at ts+0..ts+28 (29 elements)
    //   fp (x29)  at ts+29
    //   lr (x30)  at ts+30
    //   sp        at ts+31
    //   pc        at ts+32
    //   cpsr      at ts+33 (as u32, but may be padded)

    let user_x = |i: usize| -> usize { *ts.add(i) as usize };
    let user_fp = *ts.add(29) as usize;
    let user_lr = *ts.add(30) as usize;
    let user_sp = *ts.add(31) as usize;
    let user_pc = *ts.add(32) as usize;

    // Get the current UserContext
    let ctx_ptr = CURRENT_CTX.with(|c| c.get());
    if ctx_ptr.is_null() {
        std::process::abort();
    }
    let context = &mut *ctx_ptr;

    // Populate UserContext fields.
    // trapframe::UserContext on aarch64:
    //   trap_num: usize,      // offset 0*8
    //   __reserved: usize,    // offset 1*8
    //   elr: usize,           // offset 2*8
    //   spsr: usize,          // offset 3*8
    //   sp: usize,            // offset 4*8
    //   tpidr: usize,         // offset 5*8
    //   general: GeneralRegs, // offset 6*8
    //     x1..x28, x29, __reserved, x30, x0

    // trap_num = 0 for syscall (on aarch64 libos, trap_reason checks this)
    context.trap_num = 0;

    // On macOS, the mcontext PC for SIGSYS already points past
    // the svc instruction (pc = svc_addr + 4). No need to advance.
    context.elr = user_pc;

    // sp
    context.sp = user_sp;

    // tpidr (not meaningful for the kernel, but save it)
    context.tpidr = 0;

    // General registers -- trapframe layout has named fields, not array.
    // GeneralRegs: x1, x2, ..., x28, x29, __reserved, x30, x0
    //
    context.general.x0 = user_x(0);
    context.general.x1 = user_x(1);
    context.general.x2 = user_x(2);
    context.general.x3 = user_x(3);
    context.general.x4 = user_x(4);
    context.general.x5 = user_x(5);
    context.general.x6 = user_x(6);
    context.general.x7 = user_x(7);
    context.general.x8 = user_x(8);
    context.general.x9 = user_x(9);
    context.general.x10 = user_x(10);
    context.general.x11 = user_x(11);
    context.general.x12 = user_x(12);
    context.general.x13 = user_x(13);
    context.general.x14 = user_x(14);
    context.general.x15 = user_x(15);
    context.general.x16 = user_x(16);
    context.general.x17 = user_x(17);
    context.general.x18 = user_x(18);
    context.general.x19 = user_x(19);
    context.general.x20 = user_x(20);
    context.general.x21 = user_x(21);
    context.general.x22 = user_x(22);
    context.general.x23 = user_x(23);
    context.general.x24 = user_x(24);
    context.general.x25 = user_x(25);
    context.general.x26 = user_x(26);
    context.general.x27 = user_x(27);
    context.general.x28 = user_x(28);
    context.general.x29 = user_fp;
    context.general.x30 = user_lr;

    // Use longjmp directly from the signal handler to return to
    // the kernel. Testing shows that on macOS aarch64, longjmp from
    // a SIGSYS handler works correctly and preserves signal delivery
    // for subsequent syscalls. The previous trampoline approach
    // (modifying mcontext PC to redirect to a trampoline that does
    // longjmp) actually breaks signal delivery: macOS's sigreturn
    // path corrupts internal state, causing subsequent `svc`
    // instructions to hang instead of delivering SIGSYS.
    //
    // Since longjmp bypasses sigreturn, the kernel's SA_ONSTACK
    // state is not cleared automatically. Reset it so the next
    // signal delivery can use the alternate stack again.
    {
        let mut old_ss: nix::libc::stack_t = core::mem::zeroed();
        nix::libc::sigaltstack(core::ptr::null(), &mut old_ss);
        if old_ss.ss_flags & nix::libc::SS_ONSTACK != 0 {
            // Re-register the same stack without SS_ONSTACK to clear
            // the "currently executing on altstack" flag.
            old_ss.ss_flags = 0;
            nix::libc::sigaltstack(&old_ss, core::ptr::null_mut());
        }
    }
    KERNEL_JMP_BUF.with(|buf| {
        _aarch64_longjmp((*buf.get()).as_mut_ptr(), 1);
    });
}

// Minimal setjmp/longjmp and user jump assembly.
//
// _aarch64_setjmp: saves callee-saved registers (x19-x28, fp, lr, sp)
// _aarch64_longjmp: restores them and returns to the setjmp call site
// _aarch64_jump_to_user: loads user registers from UserContext and
//   branches to the entry point
global_asm!(
    r#"
.global _syscall_fn_entry
.set syscall_fn_entry, _syscall_fn_entry

// Dummy entry -- SIGSYS handler handles syscall interception.
// This label must exist for the linker (referenced by context.rs).
_syscall_fn_entry:
    brk #0

// Trampoline: entered when the signal handler modifies PC in the
// mcontext. At this point we are outside the signal handler and
// signal delivery is fully restored. We just call the Rust function
// that does the longjmp back to the kernel.
.global __aarch64_sigsys_trampoline
__aarch64_sigsys_trampoline:
    bl      __aarch64_do_longjmp
    brk     #1                      // should never reach here

// setjmp: save callee-saved registers
// x0 = buffer pointer (13 * 8 = 104 bytes needed)
// Returns 0 on first call, non-zero on longjmp return
.global __aarch64_setjmp
__aarch64_setjmp:
    stp     x19, x20, [x0, #0]
    stp     x21, x22, [x0, #16]
    stp     x23, x24, [x0, #32]
    stp     x25, x26, [x0, #48]
    stp     x27, x28, [x0, #64]
    stp     x29, x30, [x0, #80]
    mov     x1, sp
    str     x1, [x0, #96]
    // Save tpidr_el0 (kernel thread index)
    mrs     x1, tpidr_el0
    str     x1, [x0, #104]
    mov     x0, #0
    ret

// longjmp: restore callee-saved registers
// x0 = buffer pointer, x1 = return value
.global __aarch64_longjmp
__aarch64_longjmp:
    ldp     x19, x20, [x0, #0]
    ldp     x21, x22, [x0, #16]
    ldp     x23, x24, [x0, #32]
    ldp     x25, x26, [x0, #48]
    ldp     x27, x28, [x0, #64]
    ldp     x29, x30, [x0, #80]
    ldr     x2, [x0, #96]
    mov     sp, x2
    // Restore tpidr_el0
    ldr     x2, [x0, #104]
    msr     tpidr_el0, x2
    mov     x0, x1
    ret

// jump_to_user: load registers from UserContext and jump to elr
// x0 = pointer to UserContext
//
// UserContext layout (trapframe aarch64):
//   [0]  trap_num
//   [1]  __reserved
//   [2]  elr
//   [3]  spsr
//   [4]  sp
//   [5]  tpidr
//   [6]  general.x1
//   [7]  general.x2
//   ...
//   [33] general.x28
//   [34] general.x29
//   [35] general.__reserved
//   [36] general.x30
//   [37] general.x0
.global __aarch64_jump_to_user
__aarch64_jump_to_user:
    // x0 = pointer to UserContext
    //
    // Must load ALL guest registers (x0-x30) and jump to elr
    // without permanently clobbering any guest register.
    //
    // Strategy: push [guest_x0, guest_x30, elr] onto the user
    // stack. Load all registers except x0 and x30 from the
    // context. Then pop guest_x0, guest_x30, and branch to elr
    // using x30 as a temporary (restored immediately after).

    // Set up user sp
    ldr     x1, [x0, #4*8]          // x1 = user sp (original)
    // Push guest_x0, guest_x30, elr, and original_sp below the
    // user sp. Align the scratch area to 16 bytes.
    ldr     x2, [x0, #37*8]         // x2 = guest x0
    ldr     x3, [x0, #36*8]         // x3 = guest x30
    ldr     x4, [x0, #2*8]          // x4 = elr
    sub     x5, x1, #64             // 4 slots below original sp
    and     x5, x5, #0xfffffffffffffff0  // align to 16 bytes
    stp     x2, x3, [x5]            // [+0] = guest_x0, [+8] = guest_x30
    stp     x4, x1, [x5, #16]       // [+16] = elr, [+24] = original_sp
    mov     sp, x5

    // Load general registers x1-x28, x29
    ldp     x1, x2,   [x0, #6*8]
    ldp     x3, x4,   [x0, #8*8]
    ldp     x5, x6,   [x0, #10*8]
    ldp     x7, x8,   [x0, #12*8]
    ldp     x9, x10,  [x0, #14*8]
    ldp     x11, x12, [x0, #16*8]
    ldp     x13, x14, [x0, #18*8]
    ldp     x15, x16, [x0, #20*8]
    ldp     x17, x18, [x0, #22*8]
    ldp     x19, x20, [x0, #24*8]
    ldp     x21, x22, [x0, #26*8]
    ldp     x23, x24, [x0, #28*8]
    ldp     x25, x26, [x0, #30*8]
    ldp     x27, x28, [x0, #32*8]
    ldr     x29, [x0, #34*8]        // x29 = fp

    // Now recover x0, x30, and branch to elr from the stack.
    // Use x30 temporarily to hold elr for the branch.
    ldp     x0, x30, [sp]           // x0 = guest_x0, x30 = guest_x30
    // x30 now has guest_x30. But we need to branch to elr.
    // Swap: save guest_x30 on stack, load elr into x30, branch,
    // then... no, we can't restore after branch.
    //
    // Instead: use the stack. Push guest_x30, load elr into x30,
    // then we need to restore x30 before the branch target runs.
    // This is impossible without a trampoline.
    //
    // Practical solution: use x16 as the branch target. x16 is
    // the intra-procedure-call scratch register (IP0). On aarch64,
    // the ABI says x16/x17 are used by linker veneers and may be
    // clobbered by PLT stubs. For static binaries, x16 is not
    // meaningful at function boundaries.
    //
    // Save guest x16, load elr into x16, then restore x16 from
    // the stack after branching... that's still impossible.
    //
    // Final approach: accept that x16 is clobbered. The musl
    // startup code and syscall return sites don't depend on x16
    // having a specific value (it's a scratch register).
    ldp     x16, x17, [sp, #16]     // x16 = elr, x17 = original_sp
    mov     sp, x17                  // restore user sp
    br      x16                      // jump to user code
    // Note: x16 now contains elr instead of the guest's x16.
    // This is acceptable because x16 is a scratch register per
    // the AAPCS64 calling convention.
    // Note: x16 now contains elr instead of the guest's x16.
    // This is acceptable because x16 is a scratch register per
    // the AAPCS64 calling convention.
"#
);

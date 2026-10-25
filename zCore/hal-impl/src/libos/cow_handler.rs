//! Userspace COW (Copy-on-Write) handler for libos mode.
//!
//! COW pages are mapped PROT_READ by VmMapping::map(). When user code
//! writes through the mapping, the host generates SIGSEGV (Linux) or
//! SIGBUS (macOS). This handler catches the fault, calls the Zircon
//! kernel's page fault handler to perform the COW copy, and remaps
//! the page with write permission.

use core::sync::atomic::{AtomicPtr, Ordering};

/// Callback type: takes faulting vaddr, returns true if handled.
type CowCallback = fn(vaddr: usize) -> bool;

/// Registered COW fault callback (set during init).
static COW_HANDLER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the COW fault handler callback.
pub fn register_cow_callback(cb: CowCallback) {
    COW_HANDLER.store(cb as *mut (), Ordering::Release);
}

/// Install SIGSEGV + SIGBUS handler for COW write faults.
pub fn install_cow_handler() {
    unsafe {
        let mut sa: nix::libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = cow_signal_handler as extern "C" fn(_, _, _) as usize;
        sa.sa_flags = nix::libc::SA_SIGINFO;
        nix::libc::sigemptyset(&mut sa.sa_mask);
        nix::libc::sigaction(nix::libc::SIGSEGV, &sa, core::ptr::null_mut());
        nix::libc::sigaction(nix::libc::SIGBUS, &sa, core::ptr::null_mut());
    }
}

extern "C" fn cow_signal_handler(
    sig: nix::libc::c_int,
    info: *mut nix::libc::siginfo_t,
    _uc: *mut nix::libc::c_void,
) {
    let fault_addr = unsafe { (*info).si_addr() as usize };

    // Try COW handler.
    let ptr = COW_HANDLER.load(Ordering::Acquire);
    if !ptr.is_null() {
        let cb: CowCallback = unsafe { core::mem::transmute(ptr) };
        if cb(fault_addr) {
            return; // Handled — instruction will retry.
        }
    }

    // macOS x86_64: try TLS %fs → %gs patching.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    if sig == nix::libc::SIGSEGV {
        if try_tls_patch(_uc) {
            return;
        }
    }

    // Not handled — reset to default and re-raise.
    unsafe {
        nix::libc::signal(sig, nix::libc::SIG_DFL);
        nix::libc::raise(sig);
    }
}

/// macOS x86_64: patch %fs instructions to %gs for TLS emulation.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn try_tls_patch(uc: *mut nix::libc::c_void) -> bool {
    #[repr(C)]
    struct Ucontext {
        uc_onstack: i32,
        uc_sigmask: u32,
        uc_stack: [u32; 5],
        uc_link: usize,
        uc_mcsize: usize,
        uc_mcontext: *const Mcontext,
    }
    #[repr(C)]
    struct Mcontext {
        _pad: [u64; 10], // trapno..rbp
        _rsp: u64,
        _r8_to_r15: [u64; 8],
        rip: u64,
    }
    unsafe {
        let uc = uc as *mut Ucontext;
        let mut rip = (*(*uc).uc_mcontext).rip as *mut u8;
        while rip.read() == 0x66 {
            rip = rip.add(1);
        }
        match rip.read() {
            0x64 => {
                rip.write(0x65);
                true
            }
            0x65 => true,
            _ => false,
        }
    }
}

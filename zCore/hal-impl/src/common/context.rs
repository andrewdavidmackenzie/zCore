//! User context.

use cfg_if::cfg_if;
use core::fmt;
use trapframe::UserContext as UserContextInner;

pub use trapframe::GeneralRegs;

cfg_if! {
    if #[cfg(all(feature = "libos", any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_os = "linux"),
    )))] {
        pub use trapframe::syscall_fn_entry as syscall_entry;
    } else if #[cfg(all(feature = "libos", target_arch = "aarch64", target_os = "macos"))] {
        pub use crate::imp::aarch64_macos_fncall::syscall_fn_entry as syscall_entry;
    } else {
        pub use dummpy_syscall_entry as syscall_entry;
        pub fn dummpy_syscall_entry() {
            unreachable!("dummpy_syscall_entry")
        }
    }
}

// Re-export from hal crate.
pub use hal::{TrapReason, UserContextField};

cfg_if! {
    if #[cfg(not(feature = "libos"))] {
        pub const TIMER_INTERRUPT_VEC: usize = crate::timer_interrupt_vector();
    } else {
        /// Dummy value -- libos mode has no hardware timer interrupts.
        pub const TIMER_INTERRUPT_VEC: usize = usize::MAX;
    }
}

// TrapReason constructors moved to per-arch modules:
//   bare/arch/x86_64/trap.rs::trap_reason_from()
//   bare/arch/aarch64/trap.rs::trap_reason_from()
//   bare/arch/riscv/trap.rs::trap_reason_from()

// Unused after move -- remove the old functions.
// The trap_reason() method below uses cfg_if to call the per-arch
// versions directly via crate::imp::arch::trap::trap_reason_from().

/// User context saved on trap.
///
/// On x86_64, includes FPU/SSE vector registers (saved/restored via
/// FXSAVE/FXRSTOR around user-mode transitions).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct UserContext {
    inner: UserContextInner,
    #[cfg(target_arch = "x86_64")]
    pub vector_regs: VectorRegs,
}

impl UserContext {
    /// Create an empty user context.
    pub fn new() -> Self {
        Self {
            inner: UserContextInner::default(),
            #[cfg(target_arch = "x86_64")]
            vector_regs: VectorRegs::default(),
        }
    }

    /// Initialize the context for entry into userspace.
    /// Note: if the number of args < 3, please fill with zeros
    /// Eg: ctx.setup_uspace(pc_, sp_, &[arg1, arg2, 0])
    pub fn setup_uspace(&mut self, pc: usize, sp: usize, args: &[usize; 3]) {
        cfg_if! {
            if #[cfg(target_arch = "x86_64")] {
                self.inner.general.rip = pc;
                self.inner.general.rsp = sp;
                self.inner.general.rdi = args[0];
                self.inner.general.rsi = args[1];
                self.inner.general.rdx = args[2];
                // IOPL = 3, IF = 1
                // FIXME: set IOPL = 0 when IO port bitmap is supporte
                self.inner.general.rflags = 0x3000 | 0x200 | 0x2;
            } else if #[cfg(target_arch = "aarch64")] {
                self.inner.elr = pc;
                self.inner.sp = sp;
                self.inner.general.x0 = args[0];
                self.inner.general.x1 = args[1];
                self.inner.general.x2 = args[2];
                // Mask SError exceptions (currently unhandled).
                // TODO
                self.inner.spsr = 1 << 8;
            } else if #[cfg(target_arch = "riscv64")] {
                self.inner.sepc = pc;
                self.inner.general.sp = sp;
                self.inner.general.a0 = args[0];
                self.inner.general.a1 = args[1];
                self.inner.general.a2 = args[2];
                // SUM = 1, FS = 0b11, SPIE = 1
                self.inner.sstatus = 1 << 18 | 0b11 << 13 | 1 << 5;
            }
        }
    }

    /// Setup return addr
    pub fn set_ra(&mut self, _ra: usize) {
        cfg_if! {
            if #[cfg(target_arch = "riscv64")] {
                self.inner.general.ra = _ra;
            } else if #[cfg(target_arch = "x86_64")] {
                error!("Please set return addr via stack!");
            } else if #[cfg(target_arch = "aarch64")] {
                self.inner.general.x30 = _ra;
            } else {
                unimplemented!("Unsupported arch!");
            }
        }
    }

    /// Switch to user mode.
    ///
    /// On x86_64 (bare metal), saves/restores FPU/SSE state via
    /// FXRSTOR/FXSAVE around the user-mode transition. The kernel
    /// is compiled with SSE disabled, so only user FPU state needs
    /// to be managed.
    pub fn enter_uspace(&mut self) {
        cfg_if! {
            if #[cfg(all(feature = "libos", any(
                target_arch = "x86_64",
                all(target_arch = "aarch64", target_os = "linux"),
            )))] {
                self.inner.run_fncall()
            } else if #[cfg(all(feature = "libos", target_arch = "aarch64", target_os = "macos"))] {
                // Use vendored fncall for aarch64 macOS (trapframe doesn't support it)
                use crate::imp::aarch64_macos_fncall::UserContextFnCall;
                self.inner.run_fncall_macos()
            } else if #[cfg(target_arch = "x86_64")] {
                // Restore user FPU/SSE state before entering user mode
                unsafe {
                    core::arch::asm!(
                        "fxrstor [{}]",
                        in(reg) &self.vector_regs as *const VectorRegs,
                        options(nostack),
                    );
                }
                self.inner.run();
                // Save user FPU/SSE state after returning from user mode
                unsafe {
                    core::arch::asm!(
                        "fxsave [{}]",
                        in(reg) &mut self.vector_regs as *mut VectorRegs,
                        options(nostack),
                    );
                }
            } else {
                self.inner.run()
            }
        }
    }

    /// Returns the `error_code` field of the context.
    #[cfg(any(target_arch = "x86_64", doc))]
    pub fn error_code(&self) -> usize {
        self.inner.error_code
    }

    /// Returns [`TrapReason`] according to the context.
    pub fn trap_reason(&self) -> TrapReason {
        cfg_if! {
            if #[cfg(feature = "libos")] {
                // In libos mode, all traps come from the signal-based
                // trapframe (SIGSYS for syscalls). Hardware registers
                // (CR2, ESR_EL1, scause) are not accessible.
                let _ = self.inner.trap_num;
                TrapReason::Syscall
            } else if #[cfg(target_arch = "x86_64")] {
                crate::imp::arch::trap::trap_reason_from(self.inner.trap_num, self.inner.error_code)
            } else if #[cfg(target_arch = "aarch64")] {
                crate::imp::arch::trap::trap_reason_from(self.inner.trap_num)
            } else if #[cfg(target_arch = "riscv64")] {
                crate::imp::arch::trap::trap_reason_from(riscv::register::scause::read())
            } else {
                unimplemented!()
            }
        }
    }
    /// Returns a `usize` representing the trap reason. (i.e., IDT vector for x86, `scause` for RISC-V)
    pub fn raw_trap_reason(&self) -> usize {
        cfg_if! {
            if #[cfg(target_arch = "x86_64")] {
                self.inner.trap_num
            } else if #[cfg(target_arch = "aarch64")] {
                unimplemented!() // ESR_EL1
            } else if #[cfg(target_arch = "riscv64")] {
                riscv::register::scause::read().bits()
            } else {
                unimplemented!()
            }
        }
    }

    /// Returns the reference of general registers.
    pub fn general(&self) -> &GeneralRegs {
        &self.inner.general
    }

    /// Returns the mutable reference of general registers.
    pub fn general_mut(&mut self) -> &mut GeneralRegs {
        &mut self.inner.general
    }

    fn field_ref(&mut self, which: UserContextField) -> &mut usize {
        cfg_if! {
            if #[cfg(target_arch = "x86_64")] {
                match which {
                    UserContextField::InstrPointer => &mut self.inner.general.rip,
                    UserContextField::StackPointer => &mut self.inner.general.rsp,
                    UserContextField::ThreadPointer => &mut self.inner.general.fsbase,
                    UserContextField::ReturnValue => &mut self.inner.general.rax,
                }
            } else if #[cfg(target_arch = "aarch64")] {
                match which {
                    UserContextField::InstrPointer => &mut self.inner.elr,
                    UserContextField::StackPointer => &mut self.inner.sp,
                    UserContextField::ThreadPointer => &mut self.inner.tpidr,
                    UserContextField::ReturnValue => &mut self.inner.general.x0,
                }
            } else if #[cfg(target_arch = "riscv64")] {
                match which {
                    UserContextField::InstrPointer => &mut self.inner.sepc,
                    UserContextField::StackPointer => &mut self.inner.general.sp,
                    UserContextField::ThreadPointer => &mut self.inner.general.tp,
                    UserContextField::ReturnValue => &mut self.inner.general.a0,
                }
            } else {
                unimplemented!()
            }
        }
    }

    /// Read a field of the context.
    pub fn get_field(&mut self, which: UserContextField) -> usize {
        *self.field_ref(which)
    }

    /// Write a field of the context.
    pub fn set_field(&mut self, which: UserContextField, value: usize) {
        *self.field_ref(which) = value;
    }

    /// Advance the instruction pointer in trap handler on some architecture.
    pub fn advance_pc(&mut self, reason: TrapReason) {
        cfg_if! {
            if #[cfg(target_arch = "riscv64")] {
                if let TrapReason::Syscall = reason { self.inner.sepc += 4 }
            } else {
                let _ = reason;
            }
        }
    }
}

impl Default for UserContext {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for UserContext {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        /// X86 vector registers.
        #[repr(C, align(16))]
        #[derive(Debug, Copy, Clone)]
        pub struct VectorRegs {
            pub fcw: u16,
            pub fsw: u16,
            pub ftw: u8,
            pub _pad0: u8,
            pub fop: u16,
            pub fip: u32,
            pub fcs: u16,
            pub _pad1: u16,

            pub fdp: u32,
            pub fds: u16,
            pub _pad2: u16,
            pub mxcsr: u32,
            pub mxcsr_mask: u32,

            pub mm: [U128; 8],
            pub xmm: [U128; 16],
            pub reserved: [U128; 3],
            pub available: [U128; 3],
        }

        // https://xem.github.io/minix86/manual/intel-x86-and-64-manual-vol1/o_7281d5ea06a5b67a-274.html
        impl Default for VectorRegs {
            fn default() -> Self {
                VectorRegs {
                    fcw: 0x37f,
                    mxcsr: 0x1f80,
                    ..unsafe { core::mem::zeroed() }
                }
            }
        }

        // workaround: libcore has bug on Debug print u128 ??
        #[derive(Default, Clone, Copy)]
        #[repr(C, align(16))]
        pub struct U128(pub [u64; 2]);

        impl fmt::Debug for U128 {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:#016x}_{:016x}", self.0[1], self.0[0])
            }
        }
    }
}

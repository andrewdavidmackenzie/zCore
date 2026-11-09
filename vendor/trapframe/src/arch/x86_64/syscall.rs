use super::UserContext;
use core::arch::global_asm;
use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::registers::control::{Cr4, Cr4Flags};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

global_asm!(include_str!("syscall.S"));

/// Whether the CPU supports FSGSBASE instructions (rdfsbase, wrfsbase,
/// rdgsbase, wrgsbase). Detected via CPUID at init time. 1 = yes, 0 = no.
/// Read from assembly in syscall.S to choose fast-path vs MSR fallback.
#[no_mangle]
pub static TRAPFRAME_FSGSBASE: AtomicU8 = AtomicU8::new(0);

/// Check whether FSGSBASE instructions are available on this CPU.
pub fn has_fsgsbase() -> bool {
    TRAPFRAME_FSGSBASE.load(Ordering::Relaxed) != 0
}

pub fn init() {
    let cpuid = raw_cpuid::CpuId::new();
    unsafe {
        // enable `syscall` instruction
        assert!(cpuid
            .get_extended_processor_and_feature_identifiers()
            .unwrap()
            .has_syscall_sysret());
        Efer::update(|efer| {
            efer.insert(EferFlags::SYSTEM_CALL_EXTENSIONS);
        });

        // enable `FSGSBASE` instructions if supported
        let fsgsbase = cpuid
            .get_extended_feature_info()
            .is_some_and(|f| f.has_fsgsbase());
        if fsgsbase {
            Cr4::update(|cr4| {
                cr4.insert(Cr4Flags::FSGSBASE);
            });
            TRAPFRAME_FSGSBASE.store(1, Ordering::Relaxed);
        }

        // flags to clear on syscall
        // copy from Linux 5.0
        // TF|DF|IF|IOPL|AC|NT
        const RFLAGS_MASK: u64 = 0x47700;

        LStar::write(VirtAddr::new(syscall_entry as *const () as usize as u64));
        SFMask::write(RFlags::from_bits(RFLAGS_MASK).unwrap());
    }
}

extern "sysv64" {
    fn syscall_entry();
    fn syscall_return(regs: &mut UserContext);
}

impl UserContext {
    /// Go to user space with the context, and come back when a trap occurs.
    ///
    /// On return, the context will be reset to the status before the trap.
    /// Trap reason and error code will be placed at `trap_num` and `error_code`.
    ///
    /// If the trap was triggered by `syscall` instruction, the `trap_num` will be set to `0x100`.
    ///
    /// If `trap_num` is `0x100`, it will go user by `sysret` (`rcx` and `r11` are dropped),
    /// otherwise it will use `iret`.
    ///
    /// # Example
    /// ```no_run
    /// use trapframe::{UserContext, GeneralRegs};
    ///
    /// // init user space context
    /// let mut context = UserContext {
    ///     general: GeneralRegs {
    ///         rip: 0x1000,
    ///         rsp: 0x10000,
    ///         ..Default::default()
    ///     },
    ///     ..Default::default()
    /// };
    /// // go to user
    /// context.run();
    /// // back from user
    /// println!("back from user: {:#x?}", context);
    /// ```
    pub fn run(&mut self) {
        unsafe {
            syscall_return(self);
        }
    }
}

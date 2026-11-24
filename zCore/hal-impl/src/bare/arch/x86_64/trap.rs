use crate::MMUFlags;
use hal::TrapReason;
use trapframe::TrapFrame;

/// Get [`TrapReason`] from `trap_num` and `error_code` in trap frame.
pub fn trap_reason_from(trap_num: usize, error_code: usize) -> TrapReason {
    use x86::irq::*;
    const X86_INT_BASE: u8 = 0x20;
    const X86_INT_MAX: u8 = 0xff;

    if trap_num == 0x100 {
        return TrapReason::Syscall;
    }
    match trap_num as u8 {
        DEBUG_VECTOR => TrapReason::HardwareBreakpoint,
        BREAKPOINT_VECTOR => TrapReason::SoftwareBreakpoint,
        INVALID_OPCODE_VECTOR => TrapReason::UndefinedInstruction,
        ALIGNMENT_CHECK_VECTOR => TrapReason::UnalignedAccess,
        PAGE_FAULT_VECTOR => {
            bitflags::bitflags! {
                struct PageFaultErrorCode: u32 {
                    const PRESENT =     1 << 0;
                    const WRITE =       1 << 1;
                    const USER =        1 << 2;
                    const RESERVED =    1 << 3;
                    const INST =        1 << 4;
                }
            }
            let fault_vaddr = x86_64::registers::control::Cr2::read()
                .expect("invalid CR2")
                .as_u64() as _;
            let code = PageFaultErrorCode::from_bits_truncate(error_code as u32);
            let mut flags = MMUFlags::empty();
            if code.contains(PageFaultErrorCode::WRITE) {
                flags |= MMUFlags::WRITE
            } else {
                flags |= MMUFlags::READ
            }
            if code.contains(PageFaultErrorCode::USER) {
                flags |= MMUFlags::USER
            }
            if code.contains(PageFaultErrorCode::INST) {
                flags |= MMUFlags::EXECUTE
            }
            if code.contains(PageFaultErrorCode::RESERVED) {
                error!("page table entry has reserved bits set!");
            }
            TrapReason::PageFault(fault_vaddr, flags)
        }
        vec @ X86_INT_BASE..=X86_INT_MAX => TrapReason::Interrupt(vec as usize),
        _ => TrapReason::GeneralFault(trap_num),
    }
}

pub(super) const X86_INT_LOCAL_APIC_BASE: usize = 0xf0;
pub(super) const _X86_INT_APIC_SPURIOUS: usize = X86_INT_LOCAL_APIC_BASE;
pub(super) const X86_INT_APIC_TIMER: usize = X86_INT_LOCAL_APIC_BASE + 0x1;
pub(super) const _X86_INT_APIC_ERROR: usize = X86_INT_LOCAL_APIC_BASE + 0x2;

// ISA IRQ numbers
pub(super) const _X86_ISA_IRQ_PIT: usize = 0;
#[cfg(feature = "ps2-keyboard")]
pub(super) const X86_ISA_IRQ_KEYBOARD: usize = 1;
#[cfg(not(feature = "ps2-keyboard"))]
pub(super) const _X86_ISA_IRQ_KEYBOARD: usize = 1;
pub(super) const _X86_ISA_IRQ_PIC2: usize = 2;
pub(super) const X86_ISA_IRQ_COM2: usize = 3;
pub(super) const X86_ISA_IRQ_COM1: usize = 4;
pub(super) const _X86_ISA_IRQ_CMOSRTC: usize = 8;
pub(super) const _X86_ISA_IRQ_MOUSE: usize = 12;
pub(super) const _X86_ISA_IRQ_IDE: usize = 14;

/// Handle a kernel-mode breakpoint trap: fatal with a full dump
/// (routed through the same no-panic path as other kernel traps).
fn breakpoint(tf: &TrapFrame) -> ! {
    kernel_trap_fatal("kernel-mode breakpoint", tf)
}

pub(super) fn super_timer() {
    crate::timer::timer_tick();
}

/// Attempt recovery from a kernel-mode #GP by skipping the faulting
/// instruction when it is an MSR access (`rdmsr` = `0F 32`,
/// `wrmsr` = `0F 30`).
///
/// #GP from these two instructions means the MSR does not exist on
/// this CPU — e.g. x2APIC MSRs touched before enabling x2APIC mode,
/// or model-specific registers absent under a particular
/// virtualization configuration. Both instructions are fixed 2-byte
/// encodings, so the recovery is well-defined: advance RIP past the
/// instruction and, for reads, zero the result registers (the MSR
/// "reads as zero"). The system keeps running (with an error logged)
/// instead of parking the CPU in a panic loop.
///
/// This is deliberately narrow: only MSR accesses are skipped, only
/// in kernel mode. Any other #GP is still fatal — skipping an
/// arbitrary faulting instruction is unsafe.
///
/// Returns `true` if recovered (`tf` updated; the caller returns and
/// the trap.S restore path resumes past the instruction).
fn try_recover_kernel_gp(tf: &mut TrapFrame) -> bool {
    if tf.trap_num != 0x0d {
        return false;
    }
    // The instruction bytes at the faulting RIP. Kernel text is
    // always mapped — a code-fetch fault would be a #PF, not #GP —
    // so this read is safe.
    let code = unsafe { core::slice::from_raw_parts(tf.rip as *const u8, 2) };
    let cpu = super::cpu::cpu_id();
    match code {
        [0x0f, 0x32] => {
            // rdmsr: ECX holds the MSR index at fault time; the
            // result registers are undefined on #GP — zero them.
            error!(
                "rdmsr({:#x}) #GP at rip={:#x} on CPU{}: MSR not present — \
                 skipping instruction, result zeroed",
                tf.rcx, tf.rip, cpu
            );
            tf.rax = 0;
            tf.rdx = 0;
            tf.rip += 2;
            true
        }
        [0x0f, 0x30] => {
            // wrmsr: the write simply did not happen.
            error!(
                "wrmsr({:#x}) #GP at rip={:#x} on CPU{}: MSR not present — \
                 skipping instruction (write dropped)",
                tf.rcx, tf.rip, cpu
            );
            tf.rip += 2;
            true
        }
        _ => false,
    }
}

/// Handle an unrecoverable kernel-mode trap: dump the complete frame
/// through the early console, kill the current thread, and halt this
/// CPU.
///
/// The kernel never panics here. The dump goes through the direct
/// early-console path (no log filtering, no console-lock contention)
/// BEFORE anything else — that is where messages were lost
/// historically (the KVM x2APIC #GP: 7 CPUs parked with no output).
///
/// After the dump, [`KernelHandler::handle_kernel_trap_fatal`] lets
/// the kernel kill the current thread so the rest of the system
/// observes the death (waiters woken, crashsvc runs). Then this CPU
/// halts: there is no safe way to unwind a kernel-mode fault through
/// possibly-held locks, and returning would re-execute the faulting
/// instruction forever. Every other CPU keeps running.
fn kernel_trap_fatal(reason: &'static str, tf: &TrapFrame) -> ! {
    // Instruction bytes at the faulting RIP — the single most useful
    // diagnostic (identifies the faulting instruction). Kernel text
    // is always mapped (a code-fetch fault would be #PF, not #GP), so
    // a plain 8-byte read is safe.
    let mut code = [0u8; 8];
    unsafe { core::ptr::copy_nonoverlapping(tf.rip as *const u8, code.as_mut_ptr(), 8) };
    let cpu = super::cpu::cpu_id();
    crate::console::console_write_fmt(format_args!(
        "\n[ktrap] CPU{} {}: trap_num={:#x} error_code={:#x}\n\
         [ktrap] rip={:#x} cs={:#x} rflags={:#x} rsp={:#x}\n\
         [ktrap] rax={:#x} rbx={:#x} rcx={:#x} rdx={:#x}\n\
         [ktrap] rsi={:#x} rdi={:#x} rbp={:#x} r8={:#x}\n\
         [ktrap] r9={:#x} r10={:#x} r11={:#x} r12={:#x}\n\
         [ktrap] r13={:#x} r14={:#x} r15={:#x}\n\
         [ktrap] code@rip: {:02x?}\n",
        cpu,
        reason,
        tf.trap_num,
        tf.error_code,
        tf.rip,
        tf.cs,
        tf.rflags,
        tf.rsp,
        tf.rax,
        tf.rbx,
        tf.rcx,
        tf.rdx,
        tf.rsi,
        tf.rdi,
        tf.rbp,
        tf.r8,
        tf.r9,
        tf.r10,
        tf.r11,
        tf.r12,
        tf.r13,
        tf.r14,
        tf.r15,
        &code,
    ));
    // Let the kernel kill the current thread (if any) so the rest of
    // the system observes the death. Must not unwind; the handler is
    // advisory.
    use hal::kernel_handler::KernelTrapInfo;
    crate::KHANDLER.handle_kernel_trap_fatal(&KernelTrapInfo {
        trap_num: tf.trap_num,
        error_code: tf.error_code,
        rip: tf.rip,
        rsp: tf.rsp,
        cpu: cpu as usize,
        reason,
    });
    // Terminal state for THIS CPU only: interrupts off, sleep forever.
    // Other CPUs keep running (and can reap the killed thread).
    crate::interrupt::intr_off();
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}

#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    trace!(
        "Interrupt: {:#x} @ CPU{}",
        tf.trap_num,
        super::cpu::cpu_id()
    );

    match trap_reason_from(tf.trap_num, tf.error_code) {
        TrapReason::HardwareBreakpoint | TrapReason::SoftwareBreakpoint => breakpoint(tf),
        TrapReason::PageFault(vaddr, flags) => {
            // On x86, the PRESENT bit in the error code distinguishes
            // unmapped pages (bit 0 = 0) from permission violations
            // (bit 0 = 1). Pass this to the user-copy fault handler.
            let is_permission_fault = (tf.error_code & 1) != 0;
            if let Some(recovery_pc) =
                crate::thread::user_copy_check_fault_with_perm(vaddr, is_permission_fault)
            {
                // Inside a guarded user-copy. Try to resolve the fault
                // first (e.g., demand-page the user data). Only redirect
                // to recovery if the page fault handler fails.
                if crate::KHANDLER.try_handle_page_fault(vaddr, flags) {
                    // Fault resolved — re-arm the guard for subsequent
                    // faults in the same multi-page copy, then resume.
                    crate::thread::user_copy_rearm(recovery_pc);
                } else {
                    // Unresolvable fault — recovery PC is already set
                    // by user_copy_check_fault; redirect execution.
                    tf.rip = recovery_pc;
                }
                return;
            }
            crate::KHANDLER.handle_page_fault(vaddr, flags);
        }
        TrapReason::Interrupt(vector) => {
            crate::interrupt::handle_irq(vector);
            if vector == X86_INT_APIC_TIMER {
                executor::set_preempt_pending();
                executor::handle_timeout();
            }
        }
        _other => {
            // If the trap came from user mode (RPL=3 in CS), forward
            // it to the kernel handler for Zircon exception delivery.
            // Only treat kernel-mode traps as fatal.
            if tf.cs & 3 == 3 {
                crate::KHANDLER.handle_user_trap(tf.trap_num, tf.error_code);
            } else {
                // Kernel mode: attempt bounded recovery (skipping MSR
                // accesses that faulted because the MSR doesn't exist
                // on this CPU) before declaring the trap fatal.
                if try_recover_kernel_gp(tf) {
                    return; // tf updated; trap.S resumes past the instruction
                }
                kernel_trap_fatal("unhandled kernel trap", tf);
            }
        }
    }
}

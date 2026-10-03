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

fn breakpoint() {
    panic!("\nEXCEPTION: Breakpoint");
}

pub(super) fn super_timer() {
    crate::timer::timer_tick();
}

#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    trace!(
        "Interrupt: {:#x} @ CPU{}",
        tf.trap_num,
        super::cpu::cpu_id()
    );

    match trap_reason_from(tf.trap_num, tf.error_code) {
        TrapReason::HardwareBreakpoint | TrapReason::SoftwareBreakpoint => breakpoint(),
        TrapReason::PageFault(vaddr, flags) => {
            // If we are inside a guarded user-copy region, the fault
            // is from kernel code reading/writing a bad user pointer.
            // Redirect execution to the recovery point instead of
            // forwarding to KHANDLER (which would panic).
            if let Some(recovery_pc) = crate::thread::user_copy_check_fault(vaddr) {
                tf.rip = recovery_pc;
                return;
            }
            crate::KHANDLER.handle_page_fault(vaddr, flags);
        }
        TrapReason::Interrupt(vector) => {
            crate::interrupt::handle_irq(vector);
            if vector == X86_INT_APIC_TIMER {
                executor::handle_timeout();
            }
        }
        other => {
            // If the trap came from user mode (RPL=3 in CS), forward
            // it to the kernel handler for Zircon exception delivery.
            // Only panic for kernel-mode traps.
            if tf.cs & 3 == 3 {
                crate::KHANDLER.handle_user_trap(tf.trap_num, tf.error_code);
            } else {
                panic!("Unhandled kernel trap {:x?} {:#x?}", other, tf);
            }
        }
    }
}

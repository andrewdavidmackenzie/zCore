use crate::thread::{get_current_thread, set_current_thread};
use crate::{IpiReason, MMUFlags};
use alloc::vec::Vec;
use hal::TrapReason;
use riscv::register::scause;
use trapframe::TrapFrame;

/// Get [`TrapReason`] from riscv scause register.
pub fn trap_reason_from(scause: scause::Scause) -> TrapReason {
    use riscv::register::scause::{Exception, Trap};
    let stval = riscv::register::stval::read();
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => TrapReason::Syscall,
        Trap::Exception(Exception::Breakpoint) => TrapReason::SoftwareBreakpoint,
        Trap::Exception(Exception::IllegalInstruction) => TrapReason::UndefinedInstruction,
        Trap::Exception(Exception::InstructionMisaligned)
        | Trap::Exception(Exception::StoreMisaligned) => TrapReason::UnalignedAccess,
        Trap::Exception(Exception::LoadPageFault) => TrapReason::PageFault(stval, MMUFlags::READ),
        Trap::Exception(Exception::StorePageFault) => TrapReason::PageFault(stval, MMUFlags::WRITE),
        Trap::Exception(Exception::InstructionPageFault) => {
            TrapReason::PageFault(stval, MMUFlags::EXECUTE)
        }
        Trap::Interrupt(_) => TrapReason::Interrupt(scause.code()),
        _ => TrapReason::GeneralFault(scause.code()),
    }
}
pub(super) const SUPERVISOR_TIMER_INT_VEC: usize = 5; // scause::Interrupt::SupervisorTimer

fn breakpoint(sepc: &mut usize) {
    info!("Exception::Breakpoint: A breakpoint set @0x{:x} ", sepc);

    // sepc holds the address of the ebreak instruction that triggered the exception.
    // Advance past it to prevent an infinite breakpoint loop when sret returns.
    *sepc += 2
}

pub(super) fn super_timer() {
    super::timer::timer_set_next();
    crate::timer::timer_tick();
    // On a supervisor timer interrupt, the instruction at sepc has not yet executed, so no sepc adjustment is needed.
}

pub(super) fn super_soft() {
    // Clear the supervisor software interrupt pending bit directly
    // via the SIP CSR (replaces deprecated sbi_rt::legacy::clear_ipi).
    unsafe {
        core::arch::asm!("csrc sip, {}", in(reg) 1 << 1); // bit 1 = SSIP
    }
    let reasons: Vec<IpiReason> = crate::interrupt::ipi_reason()
        .iter()
        .map(|x| IpiReason::from(*x))
        .collect();
    debug!("Interrupt::SupervisorSoft, reason = {:?}", reasons);
}

#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    let scause = scause::read();
    trace!("kernel trap happened: {:?}", trap_reason_from(scause));
    trace!(
        "sepc = 0x{:x} pgtoken = 0x{:x}",
        tf.sepc,
        crate::vm::current_vmtoken()
    );
    match trap_reason_from(scause) {
        TrapReason::SoftwareBreakpoint => breakpoint(&mut tf.sepc),
        TrapReason::PageFault(vaddr, flags) => crate::KHANDLER.handle_page_fault(vaddr, flags),
        TrapReason::Interrupt(vector) => {
            crate::interrupt::handle_irq(vector);
            if vector == SUPERVISOR_TIMER_INT_VEC {
                let current_thread = get_current_thread();
                set_current_thread(None);
                executor::handle_timeout();
                set_current_thread(current_thread);
            }
        }
        other => panic!("Undefined trap: {:x?} {:#x?}", other, tf),
    }
}

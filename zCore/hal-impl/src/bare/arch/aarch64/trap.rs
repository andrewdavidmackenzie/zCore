use crate::{Info, Kind, MMUFlags, Source};
use ::drivers::irq::gic_400::get_irq_num;
use cortex_a::registers::FAR_EL1;
use hal::TrapReason;
use tock_registers::interfaces::Readable;
use trapframe::TrapFrame;

/// Get [`TrapReason`] from aarch64 ESR value.
pub fn trap_reason_from(esr: usize) -> TrapReason {
    use crate::Syndrome;
    use cortex_a::registers::ESR_EL1;

    let info = Info {
        source: Source::from(esr & 0xffff),
        kind: Kind::from((esr >> 16) & 0xffff),
    };
    let esr = ESR_EL1.get() as u32;
    match info.kind {
        Kind::Synchronous => match Syndrome::from(esr) {
            Syndrome::Breakpoint | Syndrome::Brk(_) => TrapReason::SoftwareBreakpoint,
            Syndrome::Svc(_) => TrapReason::Syscall,
            Syndrome::DataAbort {
                kind: _,
                level: _,
                is_write,
            } => {
                let mut flags = MMUFlags::READ | MMUFlags::USER;
                if is_write {
                    flags |= MMUFlags::WRITE;
                }
                TrapReason::PageFault(FAR_EL1.get() as _, flags)
            }
            Syndrome::InstructionAbort { kind: _, level: _ } => {
                TrapReason::PageFault(FAR_EL1.get() as _, MMUFlags::EXECUTE | MMUFlags::USER)
            }
            Syndrome::PCAlignmentFault | Syndrome::SpAlignmentFault => TrapReason::UnalignedAccess,
            _ => TrapReason::GeneralFault(esr as usize),
        },
        Kind::Irq => TrapReason::Interrupt({
            use crate::hal_fn::mem::phys_to_virt;
            let gic_base = super::gic_base();
            let (gicc, gicd) = (
                phys_to_virt(gic_base + super::drivers::GIC_GICC_OFFSET),
                phys_to_virt(gic_base + super::drivers::GIC_GICD_OFFSET),
            );
            get_irq_num(gicc, gicd)
        }),
        _ => TrapReason::GeneralFault(esr as usize),
    }
}

#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    let info = Info {
        source: Source::from(tf.trap_num & 0xffff),
        kind: Kind::from((tf.trap_num >> 16) & 0xffff),
    };
    trace!("Exception from {:?}", info.source);
    match info.kind {
        Kind::Synchronous => {
            sync_handler(tf);
        }
        Kind::Irq | Kind::Fiq => {
            use crate::hal_fn::mem::phys_to_virt;
            let gic_base = super::gic_base();
            let irq_num = get_irq_num(
                phys_to_virt(gic_base + super::drivers::GIC_GICC_OFFSET),
                phys_to_virt(gic_base + super::drivers::GIC_GICD_OFFSET),
            );
            crate::interrupt::handle_irq(irq_num);
            // Timer IRQ: expire deadline wakeups and preempt the executor,
            // matching what riscv64 and x86_64 do in their trap handlers.
            if irq_num == super::timer_interrupt_vector() {
                crate::timer::timer_tick();
                executor::handle_timeout();
            }
        }
        _ => {
            panic!(
                "Unsupported exception type: {:?}, TrapFrame: {:?}",
                info.kind, tf
            );
        }
    }
    trace!("Exception end");
}

fn breakpoint(elr: &mut usize) {
    info!("Exception::Breakpoint: A breakpoint set @0x{:x} ", elr);
    *elr += 4;
}

fn sync_handler(tf: &mut TrapFrame) {
    match trap_reason_from(tf.trap_num) {
        TrapReason::PageFault(vaddr, flags) => crate::KHANDLER.handle_page_fault(vaddr, flags),
        TrapReason::SoftwareBreakpoint => breakpoint(&mut tf.elr),
        other => error!(
            "Unsupported trap in kernel: {:?}, FAR_EL1: {:#x?}",
            other,
            FAR_EL1.get()
        ),
    }
}

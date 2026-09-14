use crate::context::TrapReason;
use crate::{Info, Kind, Source};
use cortex_a::registers::FAR_EL1;
use kernel_drivers::irq::gic_400::get_irq_num;
use tock_registers::interfaces::Readable;
use trapframe::TrapFrame;

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
        Kind::Irq => {
            use crate::hal_fn::mem::phys_to_virt;
            let gic_base = super::gic_base();
            crate::interrupt::handle_irq(get_irq_num(
                phys_to_virt(gic_base + super::drivers::GIC_GICC_OFFSET),
                phys_to_virt(gic_base + super::drivers::GIC_GICD_OFFSET),
            ));
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
    match TrapReason::from(tf.trap_num) {
        TrapReason::PageFault(vaddr, flags) => crate::KHANDLER.handle_page_fault(vaddr, flags),
        TrapReason::SoftwareBreakpoint => breakpoint(&mut tf.elr),
        other => error!(
            "Unsupported trap in kernel: {:?}, FAR_EL1: {:#x?}",
            other,
            FAR_EL1.get()
        ),
    }
}

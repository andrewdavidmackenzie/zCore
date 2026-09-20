//! Interrupts management.
use crate::DeviceResult;
use alloc::vec::Vec;
use cortex_a::asm::wfi;

hal_fn_impl! {
    impl mod crate::hal_fn::interrupt {
        fn wait_for_interrupt() {
            intr_on();
            wfi();
            intr_off();
        }

        fn handle_irq(vector: usize) {
            crate::device_registry::all_irq().first_unwrap().handle_irq(vector);
        }

        fn intr_off() {
            unsafe {
                core::arch::asm!("msr daifset, #2");
            }
        }

        fn intr_on() {
            unsafe {
                core::arch::asm!("msr daifclr, #2");
            }
        }

        fn intr_get() -> bool {
            use cortex_a::registers::DAIF;
            use tock_registers::interfaces::Readable;
            !DAIF.is_set(DAIF::I)
        }

        fn send_ipi(cpuid: usize, reason: usize) -> DeviceResult {
            trace!("ipi [{}] => [{}]: {:x}", super::cpu::cpu_id(), cpuid, reason);
            // Push the reason into the target CPU's IPI queue
            let queue = crate::common::ipi::ipi_queue(cpuid);
            if let Some(idx) = queue.alloc_entry() {
                *queue.entry_at(idx) = reason;
                queue.commit_entry(idx);
            }
            // Send SGI 0 (Software Generated Interrupt) to the target CPU
            // via GICD_SGIR. The GIC delivers SGI 0 as IRQ 0 to the target.
            let gic_base = super::gic_base();
            let gicd_base = crate::mem::phys_to_virt(gic_base + super::drivers::GIC_GICD_OFFSET);
            let gicd_sgir = gicd_base + 0xF00; // GICD_SGIR offset
            // GICD_SGIR format: [25:24] = target list filter (0b00 = use target list)
            //                   [23:16] = CPU target list (bitmask)
            //                   [3:0]   = SGI interrupt ID (0)
            let target_mask = 1u32 << cpuid;
            let sgir_val = (target_mask << 16) as u32;
            unsafe {
                core::ptr::write_volatile(gicd_sgir as *mut u32, sgir_val);
            }
            Ok(())
        }

        fn ipi_reason() -> Vec<usize> {
            crate::common::ipi::ipi_reason()
        }
    }
}

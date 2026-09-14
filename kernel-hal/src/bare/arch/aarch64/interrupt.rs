//! Interrupts management.
use crate::HalResult;
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
            crate::drivers::all_irq().first_unwrap().handle_irq(vector);
        }

        fn intr_off() {
            unsafe {
                // Mask both IRQ and FIQ
                core::arch::asm!("msr daifset, #3");
            }
        }

        fn intr_on() {
            unsafe {
                // Unmask both IRQ and FIQ (Group 0 interrupts arrive as FIQ on Pi 400)
                core::arch::asm!("msr daifclr, #3");
            }
        }

        fn intr_get() -> bool {
            use cortex_a::registers::DAIF;
            use tock_registers::interfaces::Readable;
            !DAIF.is_set(DAIF::I)
        }

        fn send_ipi(cpuid: usize, reason: usize) -> HalResult {
            trace!("ipi [{}] => [{}]: {:x}", super::cpu::cpu_id(), cpuid, reason);
            panic!("send_ipi unsupported for aarch64");
        }

        fn ipi_reason() -> Vec<usize> {
            panic!("ipi_reason unsupported for aarch64");
        }
    }
}

//! Interrupts management.
use crate::DeviceResult;
use crate::PhysAddr;
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
            // #538: no registered IRQ controller means no further
            // interrupt delivery either — drop the vector and log
            // rather than panicking in interrupt context.
            if let Some(ctl) = crate::device_registry::all_irq().first() {
                ctl.handle_irq(vector);
            } else {
                error!("no IRQ controller registered — dropping vector {}", vector);
            }
        }

        // #538 (b): these were previously unimplemented on aarch64 —
        // the hal_fn board-contract default panicked on first use, so
        // the DDK interrupt syscalls (zx_interrupt_create and friends)
        // crashed the kernel. The GIC-400 driver (a real IrqScheme
        // implementation) sits registered in the device registry —
        // dispatch through it like x86_64 does, degrading to
        // DeviceError::NotReady if the registry is somehow empty.
        fn is_valid_irq(vector: usize) -> bool {
            crate::device_registry::all_irq()
                .first()
                .is_some_and(|ctl| ctl.is_valid_irq(vector))
        }

        fn mask_irq(vector: usize) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .mask(vector)
        }

        fn unmask_irq(vector: usize) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .unmask(vector)
        }

        fn configure_irq(
            vector: usize,
            tm: crate::device_registry::prelude::IrqTriggerMode,
            pol: crate::device_registry::prelude::IrqPolarity,
        ) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .configure(vector, tm, pol)
        }

        fn register_irq_handler(
            vector: usize,
            handler: crate::device_registry::prelude::IrqHandler,
        ) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .register_handler(vector, handler)
        }

        fn unregister_irq_handler(vector: usize) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .unregister(vector)
        }

        fn msi_alloc_block(requested_irqs: usize) -> DeviceResult<core::ops::Range<usize>> {
            // No MSI controller driver on aarch64 — the IrqScheme
            // default returns NOT_SUPPORTED, which the syscall layer
            // degrades to a software-only allocation.
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .msi_alloc_block(requested_irqs)
        }

        fn msi_free_block(block: core::ops::Range<usize>) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .msi_free_block(block)
        }

        fn msi_register_handler(
            block: core::ops::Range<usize>,
            msi_id: usize,
            handler: crate::device_registry::prelude::IrqHandler,
        ) -> DeviceResult {
            crate::device_registry::all_irq()
                .first()
                .ok_or(hal::DeviceError::NotReady)?
                .msi_register_handler(block, msi_id, handler)
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
            let gicd_base = crate::mem::phys_to_virt(PhysAddr::new(gic_base + super::drivers::GIC_GICD_OFFSET));
            let gicd_sgir = gicd_base + 0xF00; // GICD_SGIR offset
            // GICD_SGIR format: [25:24] = target list filter (0b00 = use target list)
            //                   [23:16] = CPU target list (bitmask)
            //                   [3:0]   = SGI interrupt ID (0)
            let target_mask = 1u32 << cpuid;
            let sgir_val = target_mask << 16;
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

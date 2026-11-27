//! Interrupts management.

use core::ops::Range;

use crate::device_registry::all_irq;
use crate::device_registry::prelude::{IrqHandler, IrqPolarity, IrqTriggerMode};
use crate::{DeviceError, DeviceResult};
use alloc::vec::Vec;
use x86_64::instructions::interrupts;

hal_fn_impl! {
    impl mod crate::hal_fn::interrupt {
        fn wait_for_interrupt() {
            let enable = interrupts::are_enabled();
            interrupts::enable_and_hlt();
            if !enable {
                interrupts::disable();
            }
        }

        fn is_valid_irq(gsi: usize) -> bool {
            // #538: no controller = no valid IRQs.
            all_irq().first().is_some_and(|ctl| ctl.is_valid_irq(gsi))
        }

        fn intr_on() {
            interrupts::enable();
        }

        fn intr_off() {
            interrupts::disable();
        }

        fn intr_get() -> bool {
            interrupts::are_enabled()
        }

        fn mask_irq(gsi: usize) -> DeviceResult {
            all_irq().first().ok_or(DeviceError::NotReady)?.mask(gsi)
        }

        fn unmask_irq(gsi: usize) -> DeviceResult {
            all_irq().first().ok_or(DeviceError::NotReady)?.unmask(gsi)
        }

        fn configure_irq(gsi: usize, tm: IrqTriggerMode, pol: IrqPolarity) -> DeviceResult {
            all_irq()
                .first()
                .ok_or(DeviceError::NotReady)?
                .configure(gsi, tm, pol)
        }

        fn register_irq_handler(gsi: usize, handler: IrqHandler) -> DeviceResult {
            all_irq()
                .first()
                .ok_or(DeviceError::NotReady)?
                .register_handler(gsi, handler)
        }

        fn unregister_irq_handler(gsi: usize) -> DeviceResult {
            all_irq().first().ok_or(DeviceError::NotReady)?.unregister(gsi)
        }

        fn handle_irq(vector: usize) {
            // #538: no registered IRQ controller means no further
            // interrupt delivery either — drop the vector and log
            // rather than panicking in interrupt context.
            if let Some(ctl) = all_irq().first() {
                ctl.handle_irq(vector);
            } else {
                // Review: still send the local APIC EOI — the vector
                // was acknowledged by the CPU and must be released,
                // or the local APIC blocks later interrupts at that
                // priority. (The registered-controller path sends its
                // EOI inside Apic::handle_irq.)
                ::drivers::irq::x86::Apic::local_apic().eoi();
                error!("no IRQ controller registered — dropping vector {}", vector);
            }
        }

        fn msi_alloc_block(requested_irqs: usize) -> DeviceResult<Range<usize>> {
            all_irq()
                .first()
                .ok_or(DeviceError::NotReady)?
                .msi_alloc_block(requested_irqs)
        }

        fn msi_free_block(block: Range<usize>) -> DeviceResult {
            all_irq().first().ok_or(DeviceError::NotReady)?.msi_free_block(block)
        }

        fn msi_register_handler(
            block: Range<usize>,
            msi_id: usize,
            handler: IrqHandler,
        ) -> DeviceResult {
            all_irq()
                .first()
                .ok_or(DeviceError::NotReady)?
                .msi_register_handler(block, msi_id, handler)
        }

        fn send_ipi(cpuid: usize, reason: usize) -> DeviceResult {
            trace!("ipi [{}] => [{}]: {:x}", super::cpu::cpu_id(), cpuid, reason);
            // Push the reason into the target CPU's IPI queue.
            // The queue is indexed by core index (dense 0..N), not CPU ID.
            let target_index = super::smp::apic_id_to_logical(cpuid as u8) as usize;
            let queue = crate::common::ipi::ipi_queue(target_index);
            if let Some(idx) = queue.alloc_entry() {
                *queue.entry_at(idx) = reason;
                queue.commit_entry(idx);
            }
            // Send a fixed IPI via the local APIC to the target CPU.
            // In x2APIC mode, dest is the raw APIC ID.
            // In xAPIC mode, dest is APIC ID << 24.
            let is_x2apic = raw_cpuid::CpuId::new()
                .get_feature_info()
                .is_some_and(|f| f.has_x2apic());
            let dest = if is_x2apic {
                cpuid as u32
            } else {
                (cpuid as u32) << 24
            };
            let lapic = ::drivers::irq::x86::Apic::local_apic();
            // #862: in xAPIC mode, writing the ICR while the previous
            // IPI is still in flight (Delivery Status = Send Pending)
            // silently drops the new IPI. The wake's notified bit is
            // set but the target CPU's HLT is never broken — a lost
            // wake. Poll Delivery Status until Idle before sending.
            // (x2APIC MSR writes are serialized and never dropped, so
            // this only matters in xAPIC MMIO mode.)
            if !is_x2apic {
                let mut guard = 0u32;
                while lapic.ipi_delivery_status() {
                    core::hint::spin_loop();
                    guard += 1;
                    if guard > 100_000_000 {
                        // The previous IPI is stuck — send anyway; a
                        // lost IPI is better than a hung CPU.
                        break;
                    }
                }
            }
            lapic.send_ipi(0xFE, dest);
            Ok(())
        }

        fn ipi_reason() -> Vec<usize> {
            crate::common::ipi::ipi_reason()
        }
    }
}

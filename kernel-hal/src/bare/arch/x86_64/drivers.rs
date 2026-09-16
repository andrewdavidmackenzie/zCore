use alloc::{boxed::Box, sync::Arc};

use kernel_drivers::irq::x86::Apic;
use kernel_drivers::scheme::IrqScheme;
use kernel_drivers::uart::{BufferedUart, Uart16550Pmio};
use kernel_drivers::{Device, DeviceResult};

use super::trap;
use crate::drivers;

pub(super) fn init_early() -> DeviceResult {
    let uart = Arc::new(Uart16550Pmio::new(0x3F8));
    drivers::add_device(Device::Uart(BufferedUart::new(uart)));
    let uart = Arc::new(Uart16550Pmio::new(0x2F8));
    drivers::add_device(Device::Uart(BufferedUart::new(uart)));
    Ok(())
}

pub(super) fn init() -> DeviceResult {
    // Disable the legacy 8259 PIC by masking all IRQs.
    // The PIC's default IRQ mapping (vectors 0x08-0x0F for master, 0x70-0x77 for
    // slave) conflicts with CPU exception vectors. Without masking, IRQ 0 (timer)
    // fires as vector 8 (Double Fault), causing spurious #DF in user mode.
    unsafe {
        x86::io::outb(0xA1, 0xFF); // mask all on slave PIC
        x86::io::outb(0x21, 0xFF); // mask all on master PIC
    }
    warn!("APIC: init local APIC BSP...");
    Apic::init_local_apic_bsp(crate::mem::phys_to_virt);
    warn!("APIC: parsing ACPI tables...");
    let irq = Arc::new(Apic::new(
        super::special::pc_firmware_tables().0 as usize,
        crate::mem::phys_to_virt,
    ));
    warn!("APIC: init done, setting up UART IRQs...");
    let uarts = drivers::all_uart();
    if let Some(u) = uarts.try_get(0) {
        irq.register_device(trap::X86_ISA_IRQ_COM1, u.clone().upcast())?;
        irq.unmask(trap::X86_ISA_IRQ_COM1)?;

        if let Some(u) = uarts.try_get(1) {
            irq.register_device(trap::X86_ISA_IRQ_COM2, u.clone().upcast())?;
            irq.unmask(trap::X86_ISA_IRQ_COM2)?;
        }
    }
    warn!("UART IRQs done, configuring APIC timer...");

    use x2apic::lapic::{TimerDivide, TimerMode};

    irq.register_local_apic_handler(trap::X86_INT_APIC_TIMER, Box::new(super::trap::super_timer))?;

    warn!("Measuring CPU frequency...");
    // SAFETY: this will be called once and only once for every core
    Apic::local_apic().set_timer_mode(TimerMode::Periodic);
    Apic::local_apic().set_timer_divide(TimerDivide::Div1);
    let freq = super::cpu::cpu_frequency();
    let cycles = freq as u64 * 1_000_000 / super::super::timer::TICKS_PER_SEC;
    warn!("CPU freq={} MHz, timer cycles={}", freq, cycles);
    Apic::local_apic().set_timer_initial(cycles as u32);
    Apic::local_apic().disable_timer();

    drivers::add_device(Device::Irq(irq));

    #[cfg(not(feature = "no-pci"))]
    {
        // PCI scan -- skip on real hardware for now (#269).
        warn!("PCI scan skipped (#269)");
    }

    warn!("Drivers init end.");
    Ok(())
}

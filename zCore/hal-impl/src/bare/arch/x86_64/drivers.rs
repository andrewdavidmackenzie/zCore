use alloc::{boxed::Box, sync::Arc};

use ::drivers::irq::x86::Apic;
use ::drivers::scheme::{EventScheme, IrqScheme};
use ::drivers::{Device, DeviceResult};

use super::trap;

#[cfg(feature = "uart-16550")]
use ::drivers::scheme::UartScheme;
#[cfg(feature = "uart-16550")]
use ::drivers::uart::Uart16550Pmio;

/// Create a UART device and wire its received bytes to the console
/// input buffer. The raw UART is registered directly (no BufferedUart
/// wrapper) -- console input goes through `ConsoleInput`, and console
/// output goes through `UartScheme::write_str()`.
#[cfg(feature = "uart-16550")]
fn create_uart_with_console_input(base: u16) -> Arc<dyn UartScheme> {
    let uart = Arc::new(Uart16550Pmio::new(base));
    let u = uart.clone();
    uart.subscribe(
        Box::new(move |_| {
            while let Some(c) = u.try_recv().unwrap_or(None) {
                let c = if c == b'\r' { b'\n' } else { c };
                crate::common::console::console_input_push(c);
            }
        }),
        false,
    );
    uart
}

pub(super) fn init_early() -> DeviceResult {
    #[cfg(feature = "uart-16550")]
    {
        crate::device_registry::add_device(Device::Uart(create_uart_with_console_input(0x3F8)));
        crate::device_registry::add_device(Device::Uart(create_uart_with_console_input(0x2F8)));
    }
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
    #[cfg(feature = "uart-16550")]
    {
        warn!("APIC: init done, setting up UART IRQs...");
        let uarts = crate::device_registry::all_uart();
        if let Some(u) = uarts.try_get(0) {
            irq.register_device(trap::X86_ISA_IRQ_COM1, u.clone().upcast())?;
            irq.unmask(trap::X86_ISA_IRQ_COM1)?;

            if let Some(u) = uarts.try_get(1) {
                irq.register_device(trap::X86_ISA_IRQ_COM2, u.clone().upcast())?;
                irq.unmask(trap::X86_ISA_IRQ_COM2)?;
            }
        }
    }
    // PS/2 keyboard via i8042 controller (IRQ 1).
    // Decoded keystrokes are pushed directly into the shared
    // ConsoleInput buffer via the callback.
    #[cfg(feature = "ps2-keyboard")]
    {
        use ::drivers::keyboard::Ps2Keyboard;
        use ::drivers::scheme::SchemeUpcast;
        let kbd = Arc::new(Ps2Keyboard::new(crate::common::console::console_input_push));
        irq.register_device(trap::X86_ISA_IRQ_KEYBOARD, kbd.clone().upcast())?;
        irq.unmask(trap::X86_ISA_IRQ_KEYBOARD)?;
        info!("PS/2 keyboard registered on IRQ 1");
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

    crate::device_registry::add_device(Device::Irq(irq));

    #[cfg(feature = "pci")]
    {
        use ::drivers::bus::pci;
        info!("PCI: scanning bus...");
        match pci::init(None) {
            Ok(devices) => {
                info!("PCI: found {} device(s)", devices.len());
                for dev in devices {
                    crate::device_registry::add_device(dev);
                }
            }
            Err(e) => warn!("PCI: scan failed: {:?}", e),
        }
    }

    warn!("Drivers init end.");
    Ok(())
}

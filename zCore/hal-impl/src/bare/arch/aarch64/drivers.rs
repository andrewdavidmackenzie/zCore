use crate::arch::timer::set_next_trigger;
use crate::hal_fn::mem::phys_to_virt;
use ::drivers::irq::gic_400;
use ::drivers::scheme::{EventScheme, IrqScheme, UartScheme};
use ::drivers::uart::Pl011Uart;
use ::drivers::Device;
use alloc::boxed::Box;
use alloc::sync::Arc;

/// GIC register offsets from gic_base.
///
/// QEMU virt:  GICD at base+0x0,     GICC at base+0x10000
/// RPi 400:    GICD at base+0x1000,  GICC at base+0x2000
#[cfg(not(feature = "board-raspi400"))]
pub const GIC_GICC_OFFSET: usize = 0x1_0000;
#[cfg(not(feature = "board-raspi400"))]
pub const GIC_GICD_OFFSET: usize = 0x0;

#[cfg(feature = "board-raspi400")]
pub const GIC_GICC_OFFSET: usize = 0x2000;
#[cfg(feature = "board-raspi400")]
pub const GIC_GICD_OFFSET: usize = 0x1000;

/// UART IRQ number.
/// QEMU virt: SPI 1 (GIC INTID 33).
/// RPi 400: GIC_SPI_INTERRUPT_UART0 = 121 (PL011).
#[cfg(not(feature = "board-raspi400"))]
const UART_IRQ: u32 = 33;
#[cfg(feature = "board-raspi400")]
const UART_IRQ: u32 = 121;

/// Timer IRQ number.
/// QEMU virt: physical timer PPI 14 = IRQ 30.
/// RPi 400: virtual timer PPI 11 = IRQ 27 (matching Linux).
#[cfg(not(feature = "board-raspi400"))]
const TIMER_IRQ: u32 = 30;
#[cfg(feature = "board-raspi400")]
const TIMER_IRQ: u32 = 27;

pub fn init_early() {
    let uart_base = super::uart_base();
    let gic_base = super::gic_base();
    log::info!("Drivers: UART={:#x}, GIC={:#x}", uart_base, gic_base);

    // Use firmware-configured UART (no baud rate change needed).
    // init_with_baud() disables the UART to reconfigure, which can
    // hang if the PL011 BUSY flag stays set after disable (ARM TRM:
    // BUSY reflects transmitter state, undefined after CR disable).
    #[cfg(feature = "board-raspi400")]
    let uart = Arc::new(Pl011Uart::new_crlf(phys_to_virt(uart_base)));
    #[cfg(not(feature = "board-raspi400"))]
    let uart = Arc::new(Pl011Uart::new(phys_to_virt(uart_base)));

    log::info!("PL011 UART initialized");

    // Pi 400: use non-secure Group 1 config (firmware leaves IRQs in Group 0)
    #[cfg(feature = "board-raspi400")]
    let gic = gic_400::init_nonsecure(
        phys_to_virt(gic_base + GIC_GICC_OFFSET),
        phys_to_virt(gic_base + GIC_GICD_OFFSET),
    );
    #[cfg(not(feature = "board-raspi400"))]
    let gic = gic_400::init(
        phys_to_virt(gic_base + GIC_GICC_OFFSET),
        phys_to_virt(gic_base + GIC_GICD_OFFSET),
    );
    log::info!("GIC-400 initialized");
    gic.irq_enable(TIMER_IRQ);
    gic.irq_enable(UART_IRQ);
    gic.register_handler(UART_IRQ as usize, Box::new(handle_uart_irq))
        .ok();
    gic.register_handler(TIMER_IRQ as usize, Box::new(set_next_trigger))
        .ok();
    crate::device_registry::add_device(Device::Irq(Arc::new(gic)));
    // Wire UART received bytes to the shared console input buffer.
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
    crate::device_registry::add_device(Device::Uart(uart));
}

pub fn init() {
    #[cfg(feature = "board-raspi400")]
    {
        log::info!("RPi 400: no VirtIO, skipping block device init");
        return;
    }

    #[cfg(not(feature = "board-raspi400"))]
    {
        use crate::imp::config::VIRTIO_BASE;
        use ::drivers::virtio::{MmioTransport, VirtIOHeader, VirtIoBlk};
        use core::ptr::NonNull;

        let header = NonNull::new(phys_to_virt(VIRTIO_BASE) as *mut VirtIOHeader)
            .expect("VIRTIO_BASE mapped to null");
        match unsafe { MmioTransport::new(header) } {
            Ok(transport) => match VirtIoBlk::new(transport) {
                Ok(blk) => {
                    crate::device_registry::add_device(Device::Block(Arc::new(blk)));
                }
                Err(e) => {
                    log::warn!(
                        "VirtIO block device init failed: {:?} (no block device?)",
                        e
                    );
                }
            },
            Err(e) => {
                log::warn!(
                    "VirtIO MMIO transport init failed: {:?} (no VirtIO device attached?)",
                    e
                );
            }
        }
    }
}

fn handle_uart_irq() {
    crate::device_registry::all_uart()
        .first_unwrap()
        .handle_irq(0);
}

use crate::arch::timer::set_next_trigger;
use crate::drivers;
use crate::hal_fn::mem::phys_to_virt;
use alloc::boxed::Box;
use alloc::sync::Arc;
use kernel_drivers::irq::gic_400;
use kernel_drivers::scheme::IrqScheme;
use kernel_drivers::uart::{BufferedUart, Pl011Uart};
use kernel_drivers::Device;

/// GIC register offsets from gic_base.
///
/// QEMU virt:  GICD at base+0x0,     GICC at base+0x10000
/// RPi4:       GICD at base+0x1000,  GICC at base+0x2000
#[cfg(not(feature = "board-raspi4b"))]
const GIC_GICC_OFFSET: usize = 0x1_0000;
#[cfg(not(feature = "board-raspi4b"))]
const GIC_GICD_OFFSET: usize = 0x0;

#[cfg(feature = "board-raspi4b")]
const GIC_GICC_OFFSET: usize = 0x2000;
#[cfg(feature = "board-raspi4b")]
const GIC_GICD_OFFSET: usize = 0x1000;

/// UART IRQ number.
/// QEMU virt: SPI 1 (GIC INTID 33).
/// RPi4: GIC_SPI_INTERRUPT_UART0 = 121 (from QEMU bcm2838_peripherals.h).
#[cfg(not(feature = "board-raspi4b"))]
const UART_IRQ: u32 = 33;
#[cfg(feature = "board-raspi4b")]
const UART_IRQ: u32 = 121;

/// Timer IRQ number (PPI 14 = IRQ 30 on both platforms).
const TIMER_IRQ: u32 = 30;

pub fn init_early() {
    let uart_base = super::uart_base();
    let gic_base = super::gic_base();
    log::info!("Drivers: UART={:#x}, GIC={:#x}", uart_base, gic_base);
    let uart = Pl011Uart::new(phys_to_virt(uart_base));
    let uart = Arc::new(uart);
    let gic = gic_400::init(
        phys_to_virt(gic_base + GIC_GICC_OFFSET),
        phys_to_virt(gic_base + GIC_GICD_OFFSET),
    );
    gic.irq_enable(TIMER_IRQ);
    gic.irq_enable(UART_IRQ);
    gic.register_handler(UART_IRQ as usize, Box::new(handle_uart_irq))
        .ok();
    gic.register_handler(TIMER_IRQ as usize, Box::new(set_next_trigger))
        .ok();
    drivers::add_device(Device::Irq(Arc::new(gic)));
    drivers::add_device(Device::Uart(BufferedUart::new(uart)));
}

pub fn init() {
    #[cfg(feature = "board-raspi4b")]
    {
        log::info!("RPi4: no VirtIO, skipping block device init");
        return;
    }

    #[cfg(not(feature = "board-raspi4b"))]
    {
        use crate::imp::config::VIRTIO_BASE;
        use core::ptr::NonNull;
        use kernel_drivers::virtio::{MmioTransport, VirtIOHeader, VirtIoBlk};

        let header = NonNull::new(phys_to_virt(VIRTIO_BASE) as *mut VirtIOHeader)
            .expect("VIRTIO_BASE mapped to null");
        match unsafe { MmioTransport::new(header) } {
            Ok(transport) => match VirtIoBlk::new(transport) {
                Ok(blk) => {
                    drivers::add_device(Device::Block(Arc::new(blk)));
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
    crate::drivers::all_uart().first_unwrap().handle_irq(0);
}

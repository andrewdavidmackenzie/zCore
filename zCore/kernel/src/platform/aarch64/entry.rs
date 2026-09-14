use super::consts::save_offset;
use kernel_hal::KernelConfig;

// Include the boot assembly (page table setup + MMU enable + stack setup)
#[cfg(not(feature = "board-raspi400"))]
core::arch::global_asm!(include_str!("boot.s"));
#[cfg(feature = "board-raspi400")]
core::arch::global_asm!(include_str!("boot_raspi400.s"));

// --- Board constants ---

#[cfg(not(feature = "board-raspi400"))]
mod board {
    // QEMU virt machine
    pub const PHYS_TO_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;
    pub const UART_BASE: usize = 0x0900_0000;
    pub const GIC_BASE: usize = 0x0800_0000;
    pub const FIRMWARE_TYPE: &str = "QEMU";
}

#[cfg(feature = "board-raspi400")]
mod board {
    // Raspberry Pi 400 (BCM2711)
    //   RAM at 0x0, kernel loaded at 0x80000
    //   Peripherals at 0xFE000000 (BCM2835-compatible)
    //   GIC-400 at ctrl_base(0xFF800000) + 0x40000 + offset
    pub const PHYS_TO_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;
    pub const UART_BASE: usize = 0xFE20_1000; // PL011 UART0
    pub const GIC_BASE: usize = 0xFF84_0000; // GIC base (GICD at +0x1000, GICC at +0x2000)
    pub const FIRMWARE_TYPE: &str = "RPi400";
}

/// Rust entry point, called from boot assembly after MMU is enabled.
///
/// At this point:
/// - We are running at virtual addresses
/// - The MMU is ON with identity + high mappings
/// - x0 contains the DTB pointer (from QEMU or Pi firmware)
#[no_mangle]
extern "C" fn rust_main(dtb_paddr: usize) -> ! {
    #[cfg(feature = "board-raspi400")]
    let default_cmdline = "LOG=info:ROOTPROC=/bin/sh";
    #[cfg(not(feature = "board-raspi400"))]
    let default_cmdline = "LOG=warn:ROOTPROC=/bin/busybox?sh";

    let config = KernelConfig {
        cmdline: option_env!("ZCORE_CMDLINE").unwrap_or(default_cmdline),
        firmware_type: board::FIRMWARE_TYPE,
        uart_base: board::UART_BASE,
        gic_base: board::GIC_BASE,
        phys_to_virt_offset: board::PHYS_TO_VIRT_OFFSET,
        dtb_paddr,
    };

    save_offset(board::PHYS_TO_VIRT_OFFSET);
    crate::primary_main(config);
    unreachable!()
}

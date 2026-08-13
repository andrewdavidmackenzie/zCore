use super::consts::save_offset;
use kernel_hal::KernelConfig;

// Include the boot assembly (page table setup + MMU enable + stack setup)
core::arch::global_asm!(include_str!("boot.s"));

// QEMU virt machine constants
const PHYS_TO_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;
const UART_BASE: usize = 0x0900_0000;
const GIC_BASE: usize = 0x0800_0000;

/// Rust entry point, called from boot.s after MMU is enabled.
///
/// At this point:
/// - We are running at virtual addresses (0xffff0000_4008xxxx)
/// - The MMU is ON with identity + high mappings
/// - x0 contains the DTB pointer from QEMU (currently unused)
#[no_mangle]
extern "C" fn rust_main(_dtb_ptr: usize) -> ! {
    let config = KernelConfig {
        cmdline: "LOG=info:ROOTPROC=/bin/busybox?sh",
        // Note: LOG=warn causes a kernel panic due to a layout-dependent
        // bug (likely uninitialised memory). Using LOG=info as workaround.
        // See issue #2 discussion for details.
        firmware_type: "QEMU",
        uart_base: UART_BASE,
        gic_base: GIC_BASE,
        phys_to_virt_offset: PHYS_TO_VIRT_OFFSET,
    };
    save_offset(PHYS_TO_VIRT_OFFSET);
    crate::primary_main(config);
    unreachable!()
}

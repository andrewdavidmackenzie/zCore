use crate::imp::kernel_entry;
use hal::KernelConfig;

// Include the boot assembly (page table setup + MMU enable + stack setup).
// Skipped for UEFI boot — the UEFI stub handles this.
#[cfg(all(not(feature = "board-raspi400"), not(feature = "uefi-boot")))]
core::arch::global_asm!(include_str!("boot.s"));
#[cfg(all(feature = "board-raspi400", not(feature = "uefi-boot")))]
core::arch::global_asm!(include_str!("boot_raspi400.s"));

// --- Board constants ---

#[cfg(not(feature = "board-raspi400"))]
mod board {
    pub const PHYS_TO_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;
    pub const UART_BASE: usize = 0x0900_0000;
    pub const GIC_BASE: usize = 0x0800_0000;
}

#[cfg(feature = "board-raspi400")]
mod board {
    pub const PHYS_TO_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;
    pub const UART_BASE: usize = 0xFE20_1000;
    pub const GIC_BASE: usize = 0xFF84_0000;
}

/// UEFI entry point — called by the UEFI stub after it has set up
/// page tables and enabled the MMU. The stub passes DTB paddr in x0
/// and sets up a temporary stack. This function sets up the kernel's
/// boot stack and falls through to rust_main.
///
/// The symbol is exported so the UEFI stub can find it via ELF symbol
/// table or by using a fixed offset from the kernel base.
#[no_mangle]
#[link_section = ".text"]
#[unsafe(naked)]
pub unsafe extern "C" fn rust_main_uefi(_boot_info_ptr: usize) -> ! {
    // Set up the boot stack and jump to rust_main_from_uefi.
    // Must be naked to avoid compiler-generated stack usage before
    // we switch to the kernel's boot stack.
    // x0 = pointer to UefiBootInfo (preserved)
    core::arch::naked_asm!(
        "adrp x1, boot_stack",
        "add  x1, x1, :lo12:boot_stack",
        "add  x1, x1, #0x8000",
        "mov  sp, x1",
        "b    rust_main_from_uefi",
    );
}

/// Entry point for UEFI boot — extracts boot info and calls rust_main.
#[no_mangle]
extern "C" fn rust_main_from_uefi(boot_info_ptr: usize) -> ! {
    use super::uefi_boot_info::UefiBootInfo;

    super::super::set_board_bases(board::UART_BASE, board::GIC_BASE);

    let bi = unsafe { &*(boot_info_ptr as *const UefiBootInfo) };
    if bi.magic != UefiBootInfo::MAGIC {
        // Fallback: boot_info_ptr is just dtb_paddr (old interface)
        rust_main(boot_info_ptr);
    }

    let config = KernelConfig {
        cmdline: option_env!("ZCORE_CMDLINE").unwrap_or("LOG=info"),
        phys_to_virt_offset: board::PHYS_TO_VIRT_OFFSET,
        dtb_paddr: bi.dtb_paddr as usize,
        dtb_size: bi.dtb_size as usize,
        initrd_start: bi.initrd_start,
        initrd_size: bi.initrd_size,
        ..Default::default()
    };

    unsafe { kernel_entry::primary_core_init(config) }
}

/// Rust entry point, called from boot assembly after MMU is enabled.
#[no_mangle]
extern "C" fn rust_main(dtb_paddr: usize) -> ! {
    #[cfg(feature = "board-raspi400")]
    let default_cmdline = "LOG=info:ROOTPROC=/bin/sh";
    #[cfg(not(feature = "board-raspi400"))]
    let default_cmdline = "LOG=warn:ROOTPROC=/bin/busybox?sh";

    super::super::set_board_bases(board::UART_BASE, board::GIC_BASE);

    let config = KernelConfig {
        cmdline: option_env!("ZCORE_CMDLINE").unwrap_or(default_cmdline),
        phys_to_virt_offset: board::PHYS_TO_VIRT_OFFSET,
        dtb_paddr,
        ..Default::default()
    };

    unsafe { kernel_entry::primary_core_init(config) }
}

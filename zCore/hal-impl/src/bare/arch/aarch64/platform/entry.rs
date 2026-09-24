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

// --- UEFI boot entry ---

/// UEFI entry point — called by the UEFI stub after it has set up
/// page tables and enabled the MMU. Sets up the boot stack and
/// jumps to `rust_main_from_uefi`.
#[cfg(feature = "uefi-boot")]
#[no_mangle]
#[link_section = ".text"]
#[unsafe(naked)]
pub unsafe extern "C" fn rust_main_uefi(_boot_info_ptr: usize) -> ! {
    core::arch::naked_asm!(
        "adrp x1, boot_stack",
        "add  x1, x1, :lo12:boot_stack",
        "add  x1, x1, #0x8000",
        "mov  sp, x1",
        "b    rust_main_from_uefi",
    );
}

// UEFI boot: explicit boot stack allocation.
// On raw boot, boot.s defines .bss.stack with 128 KiB.
// On UEFI boot, boot.s is excluded, so we allocate the stack here.
#[cfg(feature = "uefi-boot")]
#[link_section = ".bss.stack"]
#[used]
static UEFI_BOOT_STACK: [u8; 128 * 1024] = [0u8; 128 * 1024];

/// Entry point for UEFI boot — extracts boot info and calls
/// the common `primary_core_init` path.
#[cfg(feature = "uefi-boot")]
#[no_mangle]
extern "C" fn rust_main_from_uefi(boot_info_ptr: usize) -> ! {
    use super::uefi_boot_info::UefiBootInfo;

    super::super::set_board_bases(board::UART_BASE, board::GIC_BASE);

    let bi = unsafe { &*(boot_info_ptr as *const UefiBootInfo) };
    let config = if bi.magic == UefiBootInfo::MAGIC {
        KernelConfig {
            cmdline: option_env!("ZCORE_CMDLINE").unwrap_or("LOG=info"),
            phys_to_virt_offset: board::PHYS_TO_VIRT_OFFSET,
            dtb_paddr: bi.dtb_paddr as usize,
            dtb_size: bi.dtb_size as usize,
            initrd_start: bi.initrd_start,
            initrd_size: bi.initrd_size,
            ..Default::default()
        }
    } else {
        // Fallback: boot_info_ptr is just dtb_paddr
        KernelConfig {
            cmdline: option_env!("ZCORE_CMDLINE").unwrap_or("LOG=info"),
            phys_to_virt_offset: board::PHYS_TO_VIRT_OFFSET,
            dtb_paddr: boot_info_ptr,
            ..Default::default()
        }
    };

    unsafe { kernel_entry::primary_core_init(config) }
}

// --- Raw boot entry ---

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

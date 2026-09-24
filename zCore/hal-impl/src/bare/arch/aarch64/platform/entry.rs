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

// UEFI boot: explicit boot stack allocation.
// On raw boot, boot.s defines .bss.stack with 128 KiB.
// On UEFI boot, boot.s is excluded, so we allocate the stack here.
// rust_main_uefi sets SP to the top of this buffer.
#[cfg(feature = "uefi-boot")]
#[link_section = ".bss.stack"]
#[used]
static UEFI_BOOT_STACK: [u8; 128 * 1024] = [0u8; 128 * 1024];

// UEFI debug exception vector — dumps ESR/FAR/ELR to UART.
#[cfg(feature = "uefi-boot")]
core::arch::global_asm!(
    ".align 11",
    ".global uefi_debug_vectors",
    "uefi_debug_vectors:",
    // Current EL with SP0 (4 entries x 0x80 bytes)
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    // Current EL with SPx (4 entries x 0x80 bytes)
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    // Lower EL AArch64 (4 entries x 0x80 bytes)
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    // Lower EL AArch32 (4 entries x 0x80 bytes)
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    ".align 7",
    "b uefi_debug_exc_handler",
    "uefi_debug_exc_handler:",
    // Save frame pointer and link register for stack trace
    "mov x10, x29", // frame pointer
    "mov x11, x30", // link register
    "mrs x0, esr_el1",
    "mrs x1, far_el1",
    "mrs x2, elr_el1",
    // Write "EXC " to UART
    "ldr x3, =0xffff000009000000",
    "mov w4, #'E'",
    "strb w4, [x3]",
    "mov w4, #'X'",
    "strb w4, [x3]",
    "mov w4, #'C'",
    "strb w4, [x3]",
    "mov w4, #' '",
    "strb w4, [x3]",
    // Print ESR as hex (x0)
    "mov x5, #60", // shift start
    "1:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 2f",
    "add x6, x6, #('a' - 10)",
    "b 3f",
    "2: add x6, x6, #'0'",
    "3: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 1b",
    "mov w4, #' '",
    "strb w4, [x3]",
    // Print FAR as hex (x1)
    "mov x5, #60",
    "4:",
    "lsr x6, x1, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 5f",
    "add x6, x6, #('a' - 10)",
    "b 6f",
    "5: add x6, x6, #'0'",
    "6: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 4b",
    "mov w4, #' '",
    "strb w4, [x3]",
    // Print ELR as hex (x2)
    "mov x5, #60",
    "7:",
    "lsr x6, x2, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 8f",
    "add x6, x6, #('a' - 10)",
    "b 9f",
    "8: add x6, x6, #'0'",
    "9: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 7b",
    // Print SP_EL0 (user stack)
    "mov w4, #' '",
    "strb w4, [x3]",
    "mrs x0, sp_el0",
    "mov x5, #60",
    "20:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 21f",
    "add x6, x6, #('a' - 10)",
    "b 22f",
    "21: add x6, x6, #'0'",
    "22: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 20b",
    // Print kernel SP
    "mov w4, #' '",
    "strb w4, [x3]",
    "mov x0, sp",
    "mov x5, #60",
    "30:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 31f",
    "add x6, x6, #('a' - 10)",
    "b 32f",
    "31: add x6, x6, #'0'",
    "32: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 30b",
    // Print LR (x30 at exception)
    "mov w4, #' '",
    "strb w4, [x3]",
    "mov w4, #'L'",
    "strb w4, [x3]",
    "mov x0, x11",
    "mov x5, #60",
    "40:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 41f",
    "add x6, x6, #('a' - 10)",
    "b 42f",
    "41: add x6, x6, #'0'",
    "42: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 40b",
    // Print FP (x29 at exception)
    "mov w4, #' '",
    "strb w4, [x3]",
    "mov w4, #'F'",
    "strb w4, [x3]",
    "mov x0, x10",
    "mov x5, #60",
    "50:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 51f",
    "add x6, x6, #('a' - 10)",
    "b 52f",
    "51: add x6, x6, #'0'",
    "52: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 50b",
    "mov w4, #'\\r'",
    "strb w4, [x3]",
    "mov w4, #'\\n'",
    "strb w4, [x3]",
    // Walk frame chain: FP -> [prev_fp, ret_addr]
    // Print up to 5 return addresses
    "mov x7, x10", // x7 = current FP
    "mov x8, #5",  // max frames
    "60:",
    "cbz x7, 65f", // null FP = end
    "cbz x8, 65f", // max reached
    // Read return address at [FP+8]
    "ldr x0, [x7, #8]",
    "mov x5, #60",
    "61:",
    "lsr x6, x0, x5",
    "and x6, x6, #0xf",
    "cmp x6, #10",
    "blt 62f",
    "add x6, x6, #('a' - 10)",
    "b 63f",
    "62: add x6, x6, #'0'",
    "63: strb w6, [x3]",
    "subs x5, x5, #4",
    "bge 61b",
    "mov w4, #' '",
    "strb w4, [x3]",
    // Follow FP chain
    "ldr x7, [x7]",
    "sub x8, x8, #1",
    "b 60b",
    "65:",
    "mov w4, #'\\r'",
    "strb w4, [x3]",
    "mov w4, #'\\n'",
    "strb w4, [x3]",
    // Spin
    "10: wfe",
    "b 10b",
);

#[cfg(feature = "uefi-boot")]
pub fn install_debug_exception_vector() {
    extern "C" {
        fn uefi_debug_vectors();
    }
    unsafe {
        core::arch::asm!(
            "msr vbar_el1, {v}",
            "isb",
            v = in(reg) uefi_debug_vectors as *const () as usize,
        );
    }
}

/// Entry point for UEFI boot — extracts boot info and calls rust_main.
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

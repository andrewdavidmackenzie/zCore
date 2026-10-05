/* Linker script for petal userspace programs.
 *
 * Link base must be >= USER_ASPACE_BASE (0x200000 on x86_64,
 * 0x4_0000_0000 on aarch64) because userstart maps binaries into
 * the child process's VMAR which starts at that address. The binary
 * is non-PIE, so all absolute addresses (GOT, string literals) are
 * resolved at link time to match the expected load address.
 *
 * 0x200000 is the lowest value that works across all architectures
 * (on aarch64, any code using absolute addresses would need
 * relocations or PIC codegen; aarch64 codegen is naturally
 * PC-relative, so the base value doesn't matter in practice).
 *
 * .text and .rodata are merged into one RX segment to avoid
 * page-overlap failures in the VMAR mapper.
 * .data and .bss are in a separate RW segment for W^X safety.
 */
ENTRY(_start)

SECTIONS {
    . = 0x200000;

    .text : {
        *(.text .text.*)
        *(.rodata .rodata.*)
    }

    . = ALIGN(0x1000);

    .data : {
        *(.data .data.*)
    }

    .bss : ALIGN(8) {
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame_hdr .eh_frame .comment .note.*)
    }
}

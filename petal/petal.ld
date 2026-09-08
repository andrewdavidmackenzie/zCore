/* Linker script for petal userspace programs.
 *
 * Places code at virtual address 0. The kernel loads the ELF
 * directly with proper segment permissions via load_from_elf().
 *
 * .text and .rodata are merged into one RX segment to avoid
 * page-overlap failures in the VMAR mapper (small programs would
 * otherwise have multiple segments sharing the same 4K page).
 * .data and .bss are in a separate RW segment for W^X safety.
 */
ENTRY(_start)

SECTIONS {
    . = 0;

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

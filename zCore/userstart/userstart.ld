/* Linker script for the userstart userspace program.
 *
 * Places code at virtual address 0x10000 (above the null page).
 * The kernel maps this into the process VMAR.
 */
ENTRY(_start)

SECTIONS {
    . = 0;

    .text : {
        *(.text .text.*)
    }

    .rodata : ALIGN(8) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(8) {
        *(.data .data.*)
    }

    .bss : ALIGN(8) {
        __bss_start = .;
        *(.bss .bss.*)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame_hdr .eh_frame .comment .note.*)
    }
}

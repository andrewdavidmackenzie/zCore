/* Linker script for petal userspace programs.
 *
 * Places code at virtual address 0 for flat binary extraction.
 * The kernel maps the flat binary at the start of the process VMAR.
 */
ENTRY(_start)

SECTIONS {
    . = 0;

    .text : {
        *(.text .text.*)
    }

    .rodata : ALIGN(4) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4) {
        *(.data .data.*)
    }

    .bss : ALIGN(4) {
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame_hdr .eh_frame .comment .note.*)
    }
}

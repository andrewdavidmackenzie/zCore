/*
 * Linker script for the zCore vDSO (libzircon.so).
 *
 * Produces a shared library with two LOAD segments:
 *   PT_LOAD RX: Code (.text) - syscall trampolines
 *   PT_LOAD R:  Data (.vdso_constants) at 0x7000 - VdsoConstants
 *
 * No writable segments: the kernel patches the data page via the VMO
 * before mapping it read-only into userspace.
 */

OUTPUT_FORMAT("elf64-littleaarch64")

PHDRS
{
    text PT_LOAD FLAGS(5);   /* PF_R | PF_X = 0x5 */
    data PT_LOAD FLAGS(4);   /* PF_R = 0x4 */
    dynamic PT_DYNAMIC FLAGS(4);
}

SECTIONS
{
    /* Code segment: syscall trampolines */
    . = SIZEOF_HEADERS;
    .text : { *(.text .text.*) } :text
    .rodata : { *(.rodata .rodata.*) } :text

    .dynsym : { *(.dynsym) } :text
    .dynstr : { *(.dynstr) } :text
    .hash   : { *(.hash) } :text
    .dynamic : { *(.dynamic) } :text :dynamic

    /* VdsoConstants data page at offset 0x7000 (page 7) */
    . = 0x7000;
    .vdso_constants : { . += 4096; } :data

    /* Discard metadata */
    /DISCARD/ : {
        *(.comment)
        *(.note.*)
        *(.eh_frame*)
        *(.ARM.exidx*)
        *(.ARM.extab*)
        *(.bss .bss.*)
        *(.data .data.*)
    }
}

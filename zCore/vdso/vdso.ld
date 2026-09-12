/*
 * Linker script for the zCore vDSO (libzircon.so).
 *
 * Produces a shared library with:
 *   PT_LOAD RX: Code (.text) at 0x0000 - syscall trampolines
 *   PT_LOAD R:  Data (.vdso_constants) at 0x7000 - VdsoConstants
 *
 * All symbols are exported as dynamic symbols so the Fuchsia
 * dynamic linker can resolve them.
 */

OUTPUT_FORMAT("elf64-littleaarch64")

SECTIONS
{
    /* Code segment: syscall trampolines and wrapper functions */
    . = SIZEOF_HEADERS;
    .text : {
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .dynsym : { *(.dynsym) }
    .dynstr : { *(.dynstr) }
    .hash   : { *(.hash) }

    /* VdsoConstants data page at offset 0x7000 (page 7) */
    . = 0x7000;
    .vdso_constants : {
        PROVIDE(_vdso_constants = .);
        . += 4096;
    }

    /* Discard metadata */
    /DISCARD/ : {
        *(.comment)
        *(.note.*)
        *(.eh_frame*)
        *(.ARM.exidx*)
        *(.ARM.extab*)
    }
}

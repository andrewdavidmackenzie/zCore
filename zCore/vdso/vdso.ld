/*
 * Linker script for the zCore vDSO (libzircon.so).
 *
 * Layout matches Fuchsia's vDSO:
 *   0x0000 - 0x6FFF: Code (.text) - syscall trampolines + wrappers
 *   0x7000 - 0x7FFF: Data (.rodata.vdso_constants) - VdsoConstants
 *
 * The kernel patches the data page at boot with actual system values.
 * The entire vDSO is mapped read-only/execute into every process.
 */

SECTIONS
{
    /* Code segment: syscall trampolines and wrapper functions */
    . = 0;
    .text : {
        *(.text .text.*)
    }

    /* Read-only data (function tables, etc.) */
    .rodata : {
        *(.rodata .rodata.*)
    }

    /* VdsoConstants data page at offset 0x7000 (page 7) */
    . = 0x7000;
    .vdso_constants : {
        PROVIDE(_vdso_constants = .);
        /* Reserve space for VdsoConstants struct (~120 bytes).
         * The kernel will overwrite this with actual values at boot. */
        . += 4096;
    }

    /* Mutable data (compiler runtime support) */
    .data : {
        *(.data .data.*)
    }
    .bss : {
        *(.bss .bss.*)
    }

    /* Discard metadata sections */
    /DISCARD/ : {
        *(.comment)
        *(.note.*)
        *(.eh_frame*)
        *(.ARM.exidx*)
        *(.ARM.extab*)
        *(.got*)
        *(.plt*)
        *(.dynamic)
        *(.dynsym)
        *(.dynstr)
        *(.hash)
        *(.gnu.hash)
        *(.symtab)
        *(.strtab)
        *(.shstrtab)
    }
}

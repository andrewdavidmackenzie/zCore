OUTPUT_ARCH(riscv)
ENTRY(_start)
INCLUDE zCore/hal-impl/src/bare/arch/riscv/platform/kernel-vars.ld

/* Force the linker to keep kernel entry points even with --gc-sections.
   These are called via extern "Rust" declarations resolved at link time. */
EXTERN(primary_core_init)
EXTERN(secondary_core_init)

SECTIONS
{
    . = BASE_ADDRESS;
    start = .;

    .text : {
        stext = .;
        KEEP(*(.text.entry))
        *(.text .text.*)
        etext = .;
    }

    .rodata ALIGN(4K) : {
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        erodata = .;
    }

    .data ALIGN(4K) : {
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        edata = .;
    }

    .stack ALIGN(4K) (NOLOAD) : {
        sstack = .;
        *(.bss.stack)
    }

    .bss ALIGN(4K) (NOLOAD) : {
        bootstack = .;
        *(.bss.bootstack)
        bootstacktop = .;

        . = ALIGN(4K);
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    }

    PROVIDE(end = .);
}

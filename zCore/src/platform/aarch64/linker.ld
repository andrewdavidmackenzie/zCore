ENTRY(_boot)
BASE_ADDRESS = 0xffff000040080000;
PHYS_BASE    = 0x40080000;
PHYS_OFFSET  = BASE_ADDRESS - PHYS_BASE;

SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    /* Boot code runs before MMU is enabled, using PC-relative addressing.
       Placed first so it is at the ELF entry point / start of the binary. */
    .text.boot : AT(ADDR(.text.boot) - PHYS_OFFSET) {
        *(.text.boot)
    }

    .text : AT(ADDR(.text) - PHYS_OFFSET) {
        stext = .;
        *(.text.entry)
        *(.text .text.*)
        . = ALIGN(4K);
        etext = .;
    }

    .rodata : AT(ADDR(.rodata) - PHYS_OFFSET) {
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(4K);
        erodata = .;
    }

    .data : AT(ADDR(.data) - PHYS_OFFSET) {
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4K);
        edata = .;
    }

    .bss (NOLOAD) : AT(ADDR(.bss) - PHYS_OFFSET) {
        boot_stack = .;
        *(.bss.stack)
        . = ALIGN(4K);
        boot_stack_top = .;

        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        . = ALIGN(4K);
        ebss = .;
    }

    ekernel = .;

    /DISCARD/ : {
        *(.eh_frame) *(.eh_frame_hdr)
    }
}

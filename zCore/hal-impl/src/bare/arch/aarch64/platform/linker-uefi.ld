/* UEFI boot linker script — kernel linked at 2 GiB + 2 MiB.
 * UEFI loads applications in low memory (~0x40000000-0x50000000),
 * so the kernel at 0x80200000 avoids overlap with the UEFI stub.
 * The UEFI stub uses AllocatePages to reserve this address range
 * and loads ELF segments directly there.
 */
ENTRY(rust_main_uefi)
BASE_ADDRESS = 0xffff000080200000;
PHYS_BASE    = 0x80200000;
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

    . = ALIGN(4K);
    .data : AT(ADDR(.data) - PHYS_OFFSET) {
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4K);
        edata = .;
    }

    . = ALIGN(4K);
    .bss.stack (NOLOAD) : AT(ADDR(.bss.stack) - PHYS_OFFSET) {
        boot_stack = .;
        . += 0x20000;   /* 128 KiB boot stack (4 cores x 32 KiB) */
        . = ALIGN(4K);
        boot_stack_top = .;
    }

    .bss (NOLOAD) : AT(ADDR(.bss) - PHYS_OFFSET) {
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

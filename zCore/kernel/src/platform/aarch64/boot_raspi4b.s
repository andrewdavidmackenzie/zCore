/*
 * aarch64 Boot Assembly for Raspberry Pi 4B (BCM2711)
 *
 * This code runs immediately after the kernel is loaded at 0x80000.
 * The CPU is at EL1 (or EL2 on real Pi -- we handle both), MMU is OFF.
 *
 * On entry:
 *   x0 = DTB pointer (from Pi firmware or QEMU -dtb)
 *
 * RPi 4B memory map:
 *   0x00000000..0x3FFFFFFF = 1 GiB RAM (low)
 *   0x40000000..0xFDFFFFFF = Additional RAM (if >1G) or unused
 *   0xFE000000..0xFEFFFFFF = BCM2835-compatible peripherals
 *   0xFF000000..0xFF7FFFFF = Reserved
 *   0xFF800000..0xFFFFFFFF = ARM local peripherals + GIC
 *
 * Page table layout (1 GiB block mappings, 2-level):
 *   BOOT_PT_L1_ID / BOOT_PT_L1_HI:
 *     [0]   -> 0x00000000..0x3FFFFFFF  (1 GiB, normal memory = RAM)
 *     [1]   -> 0x40000000..0x7FFFFFFF  (1 GiB, normal memory)
 *     [2]   -> 0x80000000..0xBFFFFFFF  (1 GiB, normal memory)
 *     [3]   -> 0xC0000000..0xFFFFFFFF  (1 GiB, device memory = peripherals + GIC)
 */

.section .text.boot, "ax"
.global _boot
_boot:
    /* On real Pi 4, firmware may start at EL2. Drop to EL1 if needed. */
    mrs     x9, CurrentEL
    lsr     x9, x9, #2
    cmp     x9, #2
    b.ne    1f

    /* We are at EL2. Configure EL1 and drop down. */
    /* HCR_EL2: RW=1 (AArch64 at EL1) */
    mov     x9, #(1 << 31)
    msr     hcr_el2, x9

    /* SPSR_EL2: D/A/I/F masked, EL1h mode (0x3c5) */
    mov     x9, #0x3c5
    msr     spsr_el2, x9

    /* Return to _el1_entry at EL1 */
    adr     x9, 1f
    msr     elr_el2, x9
    eret

1:
    /* Save DTB pointer */
    mov     x20, x0

    /* ====== Set up boot page tables ====== */

    /* Zero out the 4 page tables */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    mov     x1, #4096
    bl      _zero_mem

    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    mov     x1, #4096
    bl      _zero_mem

    adrp    x0, BOOT_PT_L1_ID
    add     x0, x0, :lo12:BOOT_PT_L1_ID
    mov     x1, #4096
    bl      _zero_mem

    adrp    x0, BOOT_PT_L1_HI
    add     x0, x0, :lo12:BOOT_PT_L1_HI
    mov     x1, #4096
    bl      _zero_mem

    /* ---- L0[0] -> L1 table descriptors ---- */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    adrp    x1, BOOT_PT_L1_ID
    add     x1, x1, :lo12:BOOT_PT_L1_ID
    orr     x1, x1, #0x3
    str     x1, [x0, #0]

    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    adrp    x1, BOOT_PT_L1_HI
    add     x1, x1, :lo12:BOOT_PT_L1_HI
    orr     x1, x1, #0x3
    str     x1, [x0, #0]

    /* ---- L1 block descriptors ---- */
    /*
     * Normal memory: 0x705 = Valid | Block | AttrIndx=1(Normal) | ISH | AF
     * Device memory: 0x401 = Valid | Block | AttrIndx=0(Device) | AF
     */

    /* [0] 0x00000000 = normal memory (RAM) */
    mov     x2, #0x705

    /* [1] 0x40000000 = normal memory */
    mov     x3, #0x705
    orr     x3, x3, #0x40000000

    /* [2] 0x80000000 = normal memory */
    mov     x4, #0x705
    orr     x4, x4, #0x80000000

    /* [3] 0xC0000000 = device memory (peripherals at 0xFE000000, GIC at 0xFF840000) */
    mov     x5, #0x401
    orr     x5, x5, #0xC0000000

    /* Fill identity mapping L1 */
    adrp    x0, BOOT_PT_L1_ID
    add     x0, x0, :lo12:BOOT_PT_L1_ID
    str     x2, [x0, #0]           /* L1[0] = 0x00..0x40 normal */
    str     x3, [x0, #8]           /* L1[1] = 0x40..0x80 normal */
    str     x4, [x0, #16]          /* L1[2] = 0x80..0xC0 normal */
    str     x5, [x0, #24]          /* L1[3] = 0xC0..0xFF device */

    /* Fill high mapping L1 (same physical mappings) */
    adrp    x0, BOOT_PT_L1_HI
    add     x0, x0, :lo12:BOOT_PT_L1_HI
    str     x2, [x0, #0]
    str     x3, [x0, #8]
    str     x4, [x0, #16]
    str     x5, [x0, #24]

    /* ====== Enable FP/SIMD ====== */
    mov     x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    /* ====== Configure MMU ====== */

    /* MAIR: Attr0=Device(0x04), Attr1=Normal WB(0xFF) */
    mov     x0, #0xFF04
    msr     mair_el1, x0
    isb

    /* TCR_EL1: 48-bit VA, 4K granule, 40-bit IPS, ISH, cacheable */
    ldr     x0, =0x00000002B5103510
    msr     tcr_el1, x0
    isb

    /* TTBR0 = identity page table */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    msr     ttbr0_el1, x0

    /* TTBR1 = high page table */
    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    msr     ttbr1_el1, x0

    /* Flush TLB */
    tlbi    vmalle1
    dsb     sy
    isb

    /* Enable MMU + caches */
    mrs     x0, sctlr_el1
    orr     x0, x0, #(1 << 0)     /* M: Enable MMU */
    orr     x0, x0, #(1 << 2)     /* C: Enable D-cache */
    orr     x0, x0, #(1 << 12)    /* I: Enable I-cache */
    msr     sctlr_el1, x0
    isb

    /* ====== Jump to virtual address space ====== */
    ldr     x0, =_start_virtual
    br      x0

/* Helper: zero x1 bytes starting at x0 */
_zero_mem:
    cbz     x1, 1f
    str     xzr, [x0], #8
    sub     x1, x1, #8
    b       _zero_mem
1:  ret

.section .text.entry, "ax"
.global _start_virtual
_start_virtual:
    /* Now executing at virtual addresses */

    /* Zero BSS */
    adrp    x0, boot_stack
    add     x0, x0, :lo12:boot_stack
    adrp    x1, ebss
    add     x1, x1, :lo12:ebss
1:  cmp     x0, x1
    b.ge    2f
    str     xzr, [x0], #8
    b       1b
2:

    /* Set up the boot stack */
    adrp    x19, boot_stack_top
    add     x19, x19, :lo12:boot_stack_top
    mov     sp, x19

    /* Restore DTB pointer as first argument */
    mov     x0, x20

    /* Jump to Rust entry point */
    b       rust_main

/* ====== Page table storage ====== */
.section .data.boot_pt
.align 12
.global BOOT_PT_L0_LO
BOOT_PT_L0_LO:
    .space 4096

.align 12
.global BOOT_PT_L0_HI
BOOT_PT_L0_HI:
    .space 4096

.align 12
.global BOOT_PT_L1_ID
BOOT_PT_L1_ID:
    .space 4096

.align 12
.global BOOT_PT_L1_HI
BOOT_PT_L1_HI:
    .space 4096

/* ====== Boot stack ====== */
.section .bss.stack
.align 12
boot_stack:
    .space 0x8000   /* 32 KiB */
boot_stack_top:

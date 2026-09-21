/*
 * Raspberry Pi 400 boot assembly (AArch64).
 *
 * Entry: _boot at physical 0x80000, called by Pi firmware at EL2.
 *   - x0 = DTB pointer (physical address)
 *   - Caches may be on (firmware runs with caches enabled)
 *   - MMU is off
 *
 * Boot sequence:
 *   1. Early UART output ("zC") to confirm CPU is running
 *   2. Drop from EL2 to EL1
 *   3. Set up identity + high page tables (2-level: L0 -> L1 1GB blocks)
 *   4. Enable MMU (without caches -- firmware leaves D-cache dirty)
 *   5. Jump to virtual address, enable caches, set up stack, enter Rust
 */

.section .text.boot, "ax"
.global skernel
skernel:

.global _boot
_boot:
    /* === Early UART: PL011 at 0xFE201000 === */
    /* At 9600 baud, each char takes ~1ms. Delay ~2M cycles between chars. */
    movz    x8, #0x1000
    movk    x8, #0xFE20, lsl #16
    mov     w9, #'z'
    str     w9, [x8]
    mov     x11, #0x200000
91: sub     x11, x11, #1
    cbnz    x11, 91b
    mov     w9, #'C'
    str     w9, [x8]
    mov     x11, #0x200000
92: sub     x11, x11, #1
    cbnz    x11, 92b
    mov     w9, #'\r'
    str     w9, [x8]
    mov     x11, #0x200000
93: sub     x11, x11, #1
    cbnz    x11, 93b
    mov     w9, #'\n'
    str     w9, [x8]
    mov     x11, #0x200000
94: sub     x11, x11, #1
    cbnz    x11, 94b

    /* Drop from EL2 to EL1 if needed */
    mrs     x9, CurrentEL
    lsr     x9, x9, #2
    cmp     x9, #2
    b.ne    1f

    /* Configure EL2 before dropping to EL1 */
    mov     x9, #(1 << 31)          /* HCR_EL2: RW=1 (AArch64 at EL1) */
    msr     hcr_el2, x9

    /* Enable EL1 access to physical timer and counter */
    mov     x9, #3                  /* CNTHCTL_EL2: EL1PCEN=1, EL1PCTEN=1 */
    msr     cnthctl_el2, x9
    msr     cntvoff_el2, xzr       /* Virtual offset = 0 */

    mov     x9, #0x3c5              /* SPSR_EL2: D/A/I/F masked, EL1h */
    msr     spsr_el2, x9
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

    /* Enable MMU without caches.
       The Pi firmware leaves caches dirty. We enable MMU with both
       I-cache and D-cache off, jump to virtual, then enable caches. */
    mrs     x0, sctlr_el1
    orr     x0, x0, #(1 << 0)     /* M: Enable MMU */
    bic     x0, x0, #(1 << 2)     /* C: D-cache OFF */
    bic     x0, x0, #(1 << 12)    /* I: I-cache OFF */
    bic     x0, x0, #(1 << 19)    /* WXN: OFF — don't make writable pages XN */
    msr     sctlr_el1, x0
    isb

    /* Invalidate I-cache before jumping to virtual addresses */
    ic      iallu
    dsb     sy
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

    /* Enable caches now that we're in virtual space with stack set up */
    mrs     x0, sctlr_el1
    orr     x0, x0, #(1 << 2)     /* C: Enable D-cache */
    orr     x0, x0, #(1 << 12)    /* I: Enable I-cache */
    msr     sctlr_el1, x0
    isb

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

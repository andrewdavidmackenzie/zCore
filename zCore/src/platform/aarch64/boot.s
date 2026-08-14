/*
 * aarch64 Direct Boot Assembly
 *
 * This code runs immediately after QEMU loads the kernel via -kernel.
 * The CPU is at EL1, MMU is OFF, and we're executing at physical addresses.
 *
 * On entry:
 *   x0 = DTB pointer (from QEMU, currently unused)
 *
 * This code:
 *   1. Sets up initial page tables with:
 *      - Identity mapping of 0x00000000..0xC0000000 (first 3 GiB, covers device + 2 GiB RAM)
 *      - High mapping of 0xffff000000000000..0xffff0000C0000000 -> 0x00000000..0xC0000000
 *   2. Configures and enables the MMU
 *   3. Jumps to rust_main at its virtual (high) address
 *
 * Page table layout (1 GiB block mappings, 2-level page table):
 *   BOOT_PT_L0_LO (TTBR0, lower half):  512 entries, each covers 512 GiB
 *     [0]   -> BOOT_PT_L1_ID
 *   BOOT_PT_L0_HI (TTBR1, upper half):  512 entries, each covers 512 GiB
 *     [0]   -> BOOT_PT_L1_HI
 *   BOOT_PT_L1_ID:  512 entries, each covers 1 GiB
 *     [0]   -> 0x00000000..0x40000000  (1 GiB block, device memory)
 *     [1]   -> 0x40000000..0x80000000  (1 GiB block, normal memory)
 *     [2]   -> 0x80000000..0xC0000000  (1 GiB block, normal memory)
 *   BOOT_PT_L1_HI:  512 entries, each covers 1 GiB
 *     [0]   -> 0x00000000..0x40000000  (1 GiB block, device memory)
 *     [1]   -> 0x40000000..0x80000000  (1 GiB block, normal memory)
 *     [2]   -> 0x80000000..0xC0000000  (1 GiB block, normal memory)
 */

.section .text.boot, "ax"
.global _boot
_boot:
    /* Save DTB pointer for later use */
    mov     x20, x0

    /* ====== Set up boot page tables ====== */

    /* Zero out the page tables */
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

    /* ---- Populate BOOT_PT_L0_LO[0] -> BOOT_PT_L1_ID (table descriptor) ---- */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    adrp    x1, BOOT_PT_L1_ID
    add     x1, x1, :lo12:BOOT_PT_L1_ID
    orr     x1, x1, #0x3           /* Valid + Table descriptor */
    str     x1, [x0, #0]           /* L0[0] = &BOOT_PT_L1_ID | TABLE */

    /* ---- Populate BOOT_PT_L0_HI[0] -> BOOT_PT_L1_HI (table descriptor) ---- */
    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    adrp    x1, BOOT_PT_L1_HI
    add     x1, x1, :lo12:BOOT_PT_L1_HI
    orr     x1, x1, #0x3           /* Valid + Table descriptor */
    str     x1, [x0, #0]           /* L0[0] = &BOOT_PT_L1_HI | TABLE */

    /* ---- L1 block descriptor flags ---- */
    /*
     * Device-nGnRE memory (MAIR attr index 0):
     *   bit[0]    = 1 (Valid)
     *   bit[1]    = 0 (Block descriptor at L1)
     *   bit[2:4]  = AttrIndx = 0 (MAIR attr 0 = Device)
     *   bit[6]    = 0 (non-secure for EL1, ignored on virt)
     *   bit[7:8]  = AP = 00 (EL1 RW)
     *   bit[10]   = AF = 1 (Access Flag)
     *   = 0x401  (Valid | Block | AttrIndx=0 | AF)
     *
     * Normal memory (MAIR attr index 1):
     *   bit[0]    = 1 (Valid)
     *   bit[1]    = 0 (Block descriptor at L1)
     *   bit[2:4]  = AttrIndx = 1 (MAIR attr 1 = Normal)
     *   bit[6]    = 0
     *   bit[7:8]  = AP = 00 (EL1 RW)
     *   bit[10]   = AF = 1 (Access Flag)
     *   bit[8]    = SH = 11 (Inner Shareable) -> bits [9:8] = 0b11
     *   = 0x705  (Valid | Block | AttrIndx=1 | ISH | AF)
     */

    /* Block descriptor for device memory at 0x00000000 */
    mov     x2, #0x401             /* Valid | Block | Device(AttrIndx=0) | AF */

    /* Block descriptor for normal memory at 0x40000000 */
    mov     x3, #0x705             /* Valid | Block | Normal(AttrIndx=1) | ISH | AF */
    orr     x3, x3, #0x40000000   /* output address = 0x40000000 */

    /* Block descriptor for normal memory at 0x80000000 */
    mov     x4, #0x705             /* Valid | Block | Normal(AttrIndx=1) | ISH | AF */
    orr     x4, x4, #0x80000000   /* output address = 0x80000000 */

    /* ---- Fill identity mapping L1 (BOOT_PT_L1_ID) ---- */
    adrp    x0, BOOT_PT_L1_ID
    add     x0, x0, :lo12:BOOT_PT_L1_ID
    str     x2, [x0, #0]          /* L1[0] = 0x00000000 | Device | Block (first 1 GiB) */
    str     x3, [x0, #8]          /* L1[1] = 0x40000000 | Normal | Block (second 1 GiB) */
    str     x4, [x0, #16]         /* L1[2] = 0x80000000 | Normal | Block (third 1 GiB) */

    /* ---- Fill high mapping L1 (BOOT_PT_L1_HI) ---- */
    adrp    x0, BOOT_PT_L1_HI
    add     x0, x0, :lo12:BOOT_PT_L1_HI
    str     x2, [x0, #0]          /* L1[0] = 0x00000000 | Device | Block */
    str     x3, [x0, #8]          /* L1[1] = 0x40000000 | Normal | Block */
    str     x4, [x0, #16]         /* L1[2] = 0x80000000 | Normal | Block */

    /* ====== Enable FP/SIMD ====== */
    /* Set CPACR_EL1.FPEN = 0b11 to enable FP/SIMD at EL0 and EL1 */
    mov     x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    /* ====== Configure MMU ====== */

    /* Set MAIR_EL1 */
    /*
     * Attr0 = 0x04 (Device-nGnRE)
     * Attr1 = 0xFF (Normal, Inner/Outer Write-Back Non-Transient R/W Alloc)
     */
    mov     x0, #0xFF04
    msr     mair_el1, x0
    isb

    /* Set TCR_EL1 */
    /*
     * T0SZ  = 16 (48-bit VA for TTBR0, bits[5:0])
     * T1SZ  = 16 (48-bit VA for TTBR1, bits[21:16])
     * TG0   = 0b00 (4K granule TTBR0, bits[15:14])
     * TG1   = 0b10 (4K granule TTBR1, bits[31:30])
     * SH0   = 0b11 (Inner Shareable, bits[13:12])
     * SH1   = 0b11 (Inner Shareable, bits[29:28])
     * ORGN0 = 0b01 (WB RA WA Cacheable, bits[11:10])
     * IRGN0 = 0b01 (WB RA WA Cacheable, bits[9:8])
     * ORGN1 = 0b01 (WB RA WA Cacheable, bits[27:26])
     * IRGN1 = 0b01 (WB RA WA Cacheable, bits[25:24])
     * EPD0  = 0 (Enable TTBR0 walks, bit[7])
     * EPD1  = 0 (Enable TTBR1 walks, bit[23])
     * IPS   = 0b010 (40-bit PA, bits[34:32])
     *
     * T0SZ = 16:      0x10
     * IRGN0 = 01:     0x100
     * ORGN0 = 01:     0x400
     * SH0 = 11:       0x3000
     * TG0 = 00:       0x0
     * T1SZ = 16:      0x100000
     * IRGN1 = 01:     0x1000000
     * ORGN1 = 01:     0x4000000
     * SH1 = 11:       0x30000000
     * TG1 = 10:       0x80000000
     * IPS = 010:      0x200000000
     *
     * Total: 0x2_B510_3510
     */
    ldr     x0, =0x00000002B5103510
    msr     tcr_el1, x0
    isb

    /* Set TTBR0_EL1 (identity / lower addresses) */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    msr     ttbr0_el1, x0

    /* Set TTBR1_EL1 (kernel / upper addresses) */
    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    msr     ttbr1_el1, x0

    /* Flush TLB */
    tlbi    vmalle1
    dsb     sy
    isb

    /* Enable the MMU */
    mrs     x0, sctlr_el1
    orr     x0, x0, #(1 << 0)     /* M: Enable MMU */
    orr     x0, x0, #(1 << 2)     /* C: Enable D-cache */
    orr     x0, x0, #(1 << 12)    /* I: Enable I-cache */
    msr     sctlr_el1, x0
    isb

    /* ====== Jump to virtual address space ====== */

    /* Load the virtual address of _start_virtual and jump to it */
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
    /* Now executing at virtual addresses (0xffff0000_4008xxxx) */

    /* Zero the .bss section (required — NOLOAD means no file content) */
    adrp    x0, sbss
    add     x0, x0, :lo12:sbss
    adrp    x1, ebss
    add     x1, x1, :lo12:ebss
1:  cmp     x0, x1
    b.ge    2f
    str     xzr, [x0], #8
    b       1b
2:

    /* Set up the boot stack (inside .bss, now zeroed) */
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

/*
 * Jolla C2 / Unisoc UMS9230 boot assembly (AArch64).
 *
 * Entry: _boot at physical 0x80080000, called by U-Boot via booti.
 *   - x0 = DTB pointer (physical address)
 *   - U-Boot has display initialized (framebuffer at 0x9e000000)
 *   - U-Boot may leave caches on
 *   - MMU is off
 *   - CPU at EL2 or EL1
 *
 * Memory map (from DTB):
 *   0x00000000..0x1fffffff  — device MMIO (SoC peripherals at 0x10000000+)
 *   0x20000000..0x3fffffff  — device MMIO (UART at 0x200b0000, etc.)
 *   0x40000000..0x7fffffff  — device MMIO (WDT at 0x644e0000, etc.)
 *   0x80000000..0xbfffffff  — RAM (DRAM, 6 GiB but we map first 1 GiB)
 *                             Framebuffer at 0x9e000000 (within RAM range)
 *
 * Boot sequence:
 *   1. Write to framebuffer to confirm alive (red bar at top)
 *      NOTE: SPRD watchdog is TrustZone-protected and cannot be disabled
 *      from EL1. Hardware reset occurs ~12 s after boot.
 *   2. Drop from EL2 to EL1 if needed
 *   4. Set up identity + high page tables (2-level: L0 -> L1 1GB blocks)
 *   5. Enable MMU (without caches — U-Boot may leave D-cache dirty)
 *   6. Jump to virtual address, enable caches, set up stack, enter Rust
 *
 * Hardware watchdog: SPRD WDT at 0x644e0000, 12-second timeout.
 *   LOCK register at +0x20: write 0xE551 to unlock
 *   CTRL register at +0x08: write 0x0 to disable
 */

.section .text.boot, "ax"
.global skernel
skernel:

.global _boot
_boot:
    /* === ARM64 Image header (64 bytes) for U-Boot booti === */
    nop                         /* code0: NOP (PE/COFF compat, like Linux) */
    b       _real_entry         /* code1: branch past header */
    .quad   0x80000             /* text_offset: kernel at DRAM base + 512K */
    .quad   _kernel_image_size  /* image_size: set by linker script */
    .quad   0x02                /* flags: LE, 4K pages, place at DRAM base + text_offset */
    .quad   0                   /* res2 */
    .quad   0                   /* res3 */
    .quad   0                   /* res4 */
    .ascii  "ARM\x64"           /* magic: ARM64 image */
    .word   0                   /* res5: PE header offset (0 = not PE) */

_real_entry:
    /* === SKIP watchdog for now — may be TrustZone-protected === */
    /* TODO: re-enable once we confirm basic boot works */

    /* === Early visual feedback: write colored bars to framebuffer === */
    /* FB at 0x9e000000, ARGB8888, 720 wide, stride 2880 bytes.
       Each bar is 40 rows tall (40 * 720 = 28800 pixels) for easy
       visibility in photos. Bar N starts at row N*40. */

    /* Bar 0 (rows 0-39): RED = boot assembly entered */
    mov     x8, #0x0000
    movk    x8, #0x9e00, lsl #16        /* x8 = 0x9e000000 */
    mov     w9, #0x0000
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFF0000 = red */
    mov     x10, #(40 * 720)
95: str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 95b

    /* Bar 1 (rows 40-79): GREEN = about to check EL */
    mov     w9, #0xFF00
    movk    w9, #0xFF00, lsl #16        /* 0xFF00FF00 = green */
    mov     x10, #(40 * 720)
96: str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 96b

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

    /* Enable GICv3 system register interface at EL2.
       ICC_SRE_EL2.SRE (bit 0) = 1: enable system registers
       ICC_SRE_EL2.Enable (bit 3) = 1: allow EL1 to set ICC_SRE_EL1.SRE
       Without this, accessing ICC_SRE_EL1 from EL1 traps to EL2. */
    mrs     x9, icc_sre_el2
    orr     x9, x9, #0x1           /* SRE (bit 0) */
    orr     x9, x9, #0x8           /* Enable (bit 3) */
    msr     icc_sre_el2, x9
    isb

    mov     x9, #0x3c5              /* SPSR_EL2: D/A/I/F masked, EL1h */
    msr     spsr_el2, x9
    adr     x9, 1f
    msr     elr_el2, x9
    eret

1:
    /* Bar 2 (rows 80-119): CYAN = EL drop done */
    mov     x8, #0x8400
    movk    x8, #0x9e03, lsl #16        /* 0x9e000000 + 0x38400 */
    mov     w9, #0xFF00
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFFFF00 = cyan */
    mov     x10, #(40 * 720)
97: str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 97b

    /* Save DTB pointer */
    mov     x20, x0

    /* Ensure SCTLR_EL1 is in a known state: MMU off, caches off,
       alignment check off. U-Boot may leave arbitrary values here. */
    mrs     x9, sctlr_el1
    bic     x9, x9, #(1 << 0)      /* M: MMU off */
    bic     x9, x9, #(1 << 1)      /* A: Alignment check off */
    bic     x9, x9, #(1 << 2)      /* C: D-cache off */
    bic     x9, x9, #(1 << 12)     /* I: I-cache off */
    msr     sctlr_el1, x9
    isb

    /* ====== Set up boot page tables ====== */
    /* Each debug bar is 40 rows tall for visibility in photos. */

    /* Bar 3 (rows 120-159): MAGENTA = starting L0_LO zero */
    mov     x8, #0x4600
    movk    x8, #0x9e05, lsl #16        /* 0x9e000000 + 0x54600 */
    mov     w9, #0x00FF
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFF00FF = magenta */
    mov     x10, #(40 * 720)
98: str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 98b

    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    mov     x1, #4096
    bl      _zero_mem

    /* Bar 4 (rows 160-199): BLUE = L0_LO done, starting L0_HI */
    mov     x8, #0x0800
    movk    x8, #0x9e07, lsl #16        /* 0x9e000000 + 0x70800 */
    mov     w9, #0x00FF
    movk    w9, #0xFF00, lsl #16        /* 0xFF0000FF = blue */
    mov     x10, #(40 * 720)
99: str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 99b

    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    mov     x1, #4096
    bl      _zero_mem

    /* Bar 5 (rows 200-239): TEAL = L0_HI done, starting L1_ID */
    mov     x8, #0xca00
    movk    x8, #0x9e08, lsl #16        /* 0x9e000000 + 0x8ca00 */
    mov     w9, #0xFFFF
    movk    w9, #0xFF00, lsl #16        /* 0xFF00FFFF = teal/aqua */
    mov     x10, #(40 * 720)
100:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 100b

    adrp    x0, BOOT_PT_L1_ID
    add     x0, x0, :lo12:BOOT_PT_L1_ID
    mov     x1, #4096
    bl      _zero_mem

    /* Bar 6 (rows 240-279): ORANGE = L1_ID done, starting L1_HI */
    mov     x8, #0x8c00
    movk    x8, #0x9e0a, lsl #16        /* 0x9e000000 + 0xa8c00 */
    mov     w9, #0x00FF                     /* low half: B=0xFF, G=0x00 */
    movk    w9, #0xFFA5, lsl #16            /* 0xFFA500FF = orange */
    mov     x10, #(40 * 720)
101:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 101b

    adrp    x0, BOOT_PT_L1_HI
    add     x0, x0, :lo12:BOOT_PT_L1_HI
    mov     x1, #4096
    bl      _zero_mem

    /* ---- L0 table descriptors ---- */
    /* We need to map addresses in both the 0x00..0xFF range (devices + RAM)
       and the 0xffff_0000_xxxx range (high kernel addresses).
       L0 entry covers 512 GiB. L0[0] covers 0x00_0000_0000..0x7F_FFFF_FFFF.
       We need L0[0] for identity map, and L0[0] for high map. */

    /* Identity map: L0_LO[0] -> L1_ID table */
    adrp    x0, BOOT_PT_L0_LO
    add     x0, x0, :lo12:BOOT_PT_L0_LO
    adrp    x1, BOOT_PT_L1_ID
    add     x1, x1, :lo12:BOOT_PT_L1_ID
    orr     x1, x1, #0x3
    str     x1, [x0, #0]

    /* High map: L0_HI[0] -> L1_HI table */
    adrp    x0, BOOT_PT_L0_HI
    add     x0, x0, :lo12:BOOT_PT_L0_HI
    adrp    x1, BOOT_PT_L1_HI
    add     x1, x1, :lo12:BOOT_PT_L1_HI
    orr     x1, x1, #0x3
    str     x1, [x0, #0]

    /* ---- L1 block descriptors (1 GiB each) ---- */
    /*
     * Normal memory: 0x705 = Valid | Block | AttrIndx=1(Normal) | ISH | AF
     * Device memory: 0x401 = Valid | Block | AttrIndx=0(Device) | AF
     *
     * UMS9230 memory map:
     *   [0] 0x00000000..0x3FFFFFFF = device (GICv3 at 0x10000000)
     *   [1] 0x40000000..0x7FFFFFFF = device (WDT at 0x644e0000, UART at 0x200b0000... 
     *       wait, UART is at 0x200b0000 which is in [0]. Let's recheck.)
     *
     * Actually all peripherals are below 0x80000000:
     *   GICv3:     0x10000000
     *   UART:      0x200b0000
     *   WDT:       0x644e0000
     *   DPU:       0x31000000
     *
     *   [0] 0x00000000 = device
     *   [1] 0x40000000 = device
     *   [2] 0x80000000 = normal memory (RAM + framebuffer at 0x9e000000)
     *   [3] 0xC0000000 = normal memory (more RAM, if present)
     */

    /* [0] 0x00000000 = device memory */
    mov     x2, #0x401

    /* [1] 0x40000000 = device memory */
    mov     x3, #0x401
    orr     x3, x3, #0x40000000

    /* [2] 0x80000000 = normal memory (RAM) */
    mov     x4, #0x705
    orr     x4, x4, #0x80000000

    /* [3] 0xC0000000 = normal memory (more RAM) */
    mov     x5, #0x705
    orr     x5, x5, #0xC0000000

    /* Fill identity mapping L1 */
    adrp    x0, BOOT_PT_L1_ID
    add     x0, x0, :lo12:BOOT_PT_L1_ID
    str     x2, [x0, #0]           /* L1[0] = 0x00..0x40 device */
    str     x3, [x0, #8]           /* L1[1] = 0x40..0x80 device */
    str     x4, [x0, #16]          /* L1[2] = 0x80..0xC0 normal (RAM) */
    str     x5, [x0, #24]          /* L1[3] = 0xC0..0xFF normal (RAM) */

    /* Fill high mapping L1 (same physical mappings) */
    adrp    x0, BOOT_PT_L1_HI
    add     x0, x0, :lo12:BOOT_PT_L1_HI
    str     x2, [x0, #0]
    str     x3, [x0, #8]
    str     x4, [x0, #16]
    str     x5, [x0, #24]

    /* Bar 7 (rows 280-319): YELLOW = page tables filled */
    mov     x8, #0x4e00
    movk    x8, #0x9e0c, lsl #16        /* 0x9e000000 + 0xc4e00 */
    mov     w9, #0x00FF
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFF00FF = yellow */
    mov     x10, #(40 * 720)
102:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 102b

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
       U-Boot may leave caches dirty. Enable MMU with both
       I-cache and D-cache off, jump to virtual, then enable caches. */
    mrs     x0, sctlr_el1
    orr     x0, x0, #(1 << 0)     /* M: Enable MMU */
    bic     x0, x0, #(1 << 2)     /* C: D-cache OFF */
    bic     x0, x0, #(1 << 12)    /* I: I-cache OFF */
    bic     x0, x0, #(1 << 19)    /* WXN: OFF */
    msr     sctlr_el1, x0
    isb

    /* Invalidate I-cache before jumping to virtual addresses */
    ic      iallu
    dsb     sy
    isb

    /* Bar 8 (rows 320-359): WHITE = MMU enabled */
    mov     x8, #0x1000
    movk    x8, #0x9e0e, lsl #16        /* 0x9e000000 + 0xe1000 */
    mov     w9, #0xFFFF
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFFFFFF = white */
    mov     x10, #(40 * 720)
103:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 103b

    /* Bar 9 (rows 360-399): GREY = about to jump to virtual */
    mov     x8, #0xd200
    movk    x8, #0x9e0f, lsl #16        /* 0x9e000000 + 0xfd200 */
    mov     w9, #0x8080
    movk    w9, #0xFF80, lsl #16        /* 0xFF808080 = grey */
    mov     x10, #(40 * 720)
104:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 104b

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
    /* Now executing at virtual addresses.
       Write debug bar via identity map (TTBR0 still active). */

    /* Bar 10 (rows 400-439): PINK = virtual jump succeeded */
    mov     x8, #0x9400
    movk    x8, #0x9e11, lsl #16        /* 0x9e000000 + 0x119400 = row 400 */
    mov     w9, #0x80FF
    movk    w9, #0xFFFF, lsl #16        /* 0xFFFF80FF = pink */
    mov     x10, #(40 * 720)
105:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 105b

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

    /* Bar 11 (rows 440-479): DARK GREEN = BSS zeroed, stack set, caches on */
    mov     x8, #0x5600
    movk    x8, #0x9e13, lsl #16        /* 0x9e000000 + 0x135600 = row 440 */
    mov     w9, #0x8000
    movk    w9, #0xFF00, lsl #16        /* 0xFF008000 = dark green */
    mov     x10, #(40 * 720)
106:str     w9, [x8], #4
    sub     x10, x10, #1
    cbnz    x10, 106b

    /* Clear screen to black before entering Rust.
       FB: 720x1600 ARGB8888 at 0x9e000000 = 1,152,000 pixels.
       Flush each cache line so pixels reach the display controller. */
    mov     x8, #0x0000
    movk    x8, #0x9e00, lsl #16        /* x8 = 0x9e000000 */
    mov     w9, #0x0000
    movk    w9, #0xFF00, lsl #16        /* 0xFF000000 = opaque black */
    mov     x12, #0x9400
    movk    x12, #0x0011, lsl #16       /* x12 = 0x119400 = 1152000 (720*1600) */
107:str     w9, [x8], #4
    sub     x12, x12, #1
    /* Flush cache line every 16 pixels (64 bytes).
       Use x8-4 to target the line just written (str post-incremented x8). */
    tst     x12, #0xF
    b.ne    108f
    sub     x13, x8, #4
    dc      cvac, x13
108:cbnz    x12, 107b
    dsb     sy

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

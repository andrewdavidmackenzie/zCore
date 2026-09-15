/*
 * Custom armstub for Raspberry Pi 400 (BCM2711) with GIC-400 configuration.
 *
 * This runs at EL3 before the kernel. It:
 *   1. Configures local timer control and prescaler
 *   2. Sets L2 cache latency
 *   3. Sets CNTFRQ_EL0 to 54 MHz
 *   4. Enables FP/SIMD
 *   5. Sets SCR_EL3 (NS=1, RW=1, HCE=1, SMD=1)
 *   6. Configures GIC-400: all interrupts -> Group 1 (Non-Secure IRQ)
 *   7. Drops to Non-Secure EL2
 *
 * Based on: https://github.com/raspberrypi/tools/blob/master/armstubs/armstub8.S
 * Built with: -DGIC=1 -DBCM2711=1 (standard peripheral addresses)
 *
 * Copyright (c) 2016-2019 Raspberry Pi (Trading) Ltd.
 * Copyright (c) 2016 Stephen Warren <swarren@wwwdotorg.org>
 * SPDX-License-Identifier: BSD-3-Clause
 */

#define BIT(x) (1 << (x))

#define LOCAL_CONTROL		0xff800000
#define LOCAL_PRESCALER		0xff800008

#define GIC_DISTB		0xff841000
#define GIC_CPUB		0xff842000

#define OSC_FREQ		54000000

#define SCR_RW			BIT(10)
#define SCR_HCE			BIT(8)
#define SCR_SMD			BIT(7)
#define SCR_RES1_5		BIT(5)
#define SCR_RES1_4		BIT(4)
#define SCR_NS			BIT(0)
#define SCR_VAL \
    (SCR_RW | SCR_HCE | SCR_SMD | SCR_RES1_5 | SCR_RES1_4 | SCR_NS)

#define ACTLR_VAL \
	(BIT(0) | BIT(1) | BIT(4) | BIT(5) | BIT(6))

#define CPUECTLR_EL1		S3_1_C15_C2_1
#define CPUECTLR_EL1_SMPEN	BIT(6)

#define SPSR_EL3_D		BIT(9)
#define SPSR_EL3_A		BIT(8)
#define SPSR_EL3_I		BIT(7)
#define SPSR_EL3_F		BIT(6)
#define SPSR_EL3_MODE_EL2H	9
#define SPSR_EL3_VAL \
    (SPSR_EL3_D | SPSR_EL3_A | SPSR_EL3_I | SPSR_EL3_F | SPSR_EL3_MODE_EL2H)

#define L2CTLR_EL1		S3_1_C11_C0_2

#define GICC_CTRLR	0x0
#define GICC_PMR	0x4
#define IT_NR		0x8	// Number of interrupt enable registers (256 total irqs)
#define GICD_CTRLR	0x0
#define GICD_IGROUPR	0x80

.globl _start
_start:
	/*
	 * LOCAL_CONTROL:
	 * Bit 9 clear: Increment by 1 (vs. 2).
	 * Bit 8 clear: Timer source is 19.2MHz crystal (vs. APB).
	 */
	movz x0, #0x0000
	movk x0, #0xFF80, lsl #16	/* x0 = 0xFF800000 = LOCAL_CONTROL */
	str wzr, [x0]
	/* LOCAL_PRESCALER; divide-by (0x80000000 / register_val) == 1 */
	mov w1, 0x80000000
	str w1, [x0, #(LOCAL_PRESCALER - LOCAL_CONTROL)]

	/* Set L2 read/write cache latency to 3 */
	mrs x0, L2CTLR_EL1
	mov x1, #0x22
	orr x0, x0, x1
	msr L2CTLR_EL1, x0

	/* Set up CNTFRQ_EL0 */
	movz x0, #0x9680
	movk x0, #0x0337, lsl #16	/* x0 = 54000000 = 0x03379680 */
	msr CNTFRQ_EL0, x0

	/* Set up CNTVOFF_EL2 */
	msr CNTVOFF_EL2, xzr

	/* Enable FP/SIMD */
	/* Bit 10 (TFP) is set to 0 */
	msr CPTR_EL3, xzr

	/* Set up ACTLR */
	mov x0, #ACTLR_VAL
	msr ACTLR_EL3, x0

	/* Set SMPEN */
	mov x0, #CPUECTLR_EL1_SMPEN
	msr CPUECTLR_EL1, x0

	/* Configure GIC BEFORE setting SCR_EL3.NS=1.
	 * GICD_IGROUPR is banked by security state on GIC-400.
	 * Writes from Non-Secure state (even at EL3) are silently
	 * ignored on real hardware. We must do this while NS=0.
	 * setup_gic also writes diagnostics to 0xC0-0xC8. */
	bl      setup_gic

	/* Now set SCR_EL3 with NS=1 to enter Non-Secure world */
	mov x0, #SCR_VAL
	msr SCR_EL3, x0

	/*
	 * Set up SCTLR_EL2
	 * All set bits below are res1. LE, no WXN/I/SA/C/A/M
	 */
	movz x0, #0x0830
	movk x0, #0x30c5, lsl #16
	msr SCTLR_EL2, x0

	/* Switch to EL2 */
	mov x0, #SPSR_EL3_VAL
	msr spsr_el3, x0
	adr x0, in_el2
	msr elr_el3, x0
	eret
in_el2:

	mrs x6, MPIDR_EL1
	and x6, x6, #0x3
	cbz x6, primary_cpu

	adr x5, spin_cpu0
secondary_spin:
	wfe
	ldr x4, [x5, x6, lsl #3]
	cbz x4, secondary_spin
	mov x0, #0
	b boot_kernel

primary_cpu:
	ldr w4, kernel_entry32
	ldr w0, dtb_ptr32

boot_kernel:
	mov x1, #0
	mov x2, #0
	mov x3, #0
	br x4

.org 0xd8
.globl spin_cpu0
spin_cpu0:
	.quad 0
.org 0xe0
.globl spin_cpu1
spin_cpu1:
	.quad 0
.org 0xe8
.globl spin_cpu2
spin_cpu2:
	.quad 0
.org 0xf0
.globl spin_cpu3
spin_cpu3:
	# Shared with next two symbols/.word
	# FW clears the next 8 bytes after reading the initial value, leaving
	# the location suitable for use as spin_cpu3
.org 0xf0
.globl stub_magic
stub_magic:
	.word 0x5afe570b
.org 0xf4
.globl stub_version
stub_version:
	.word 0
.org 0xf8
.globl dtb_ptr32
dtb_ptr32:
	.word 0x0
.org 0xfc
.globl kernel_entry32
kernel_entry32:
	.word 0x0

.org 0x100

setup_gic:				// Called from secure mode - set all interrupts to group 1 and enable.
	/* Write marker 0xDEAD0001 at phys 0xC0 to prove our stub ran */
	mov	x3, #0xC0
	movz	w4, #0x0001
	movk	w4, #0xDEAD, lsl #16
	str	w4, [x3]

	/* Save SCR_EL3 at 0xC8 (before any modification) */
	mrs	x4, SCR_EL3
	str	w4, [x3, #8]		// 0xC8

	mrs	x0, MPIDR_EL1
	movz	x2, #0x1000
	movk	x2, #0xFF84, lsl #16	// x2 = 0xFF841000 = GIC_DISTB

	/* Enable group 0 AND group 1 IRQs from distributor (all cores) */
	mov	w0, #3
	str	w0, [x2, #GICD_CTRLR]
	add	x1, x2, #(GIC_CPUB - GIC_DISTB)
	mov	w0, #0x1e7
	str	w0, [x1, #GICC_CTRLR]	// Enable group 1 IRQs from CPU interface
	mov	w0, #0xff
	str	w0, [x1, #GICC_PMR]	// priority mask
	add	x2, x2, #GICD_IGROUPR
	mov	x0, #(IT_NR * 4)
	mov	w1, #~0			// group 1 all the things
3:
	subs	x0, x0, #4
	str	w1, [x2, x0]
	b.ne	3b

	/* Read back GICD_IGROUPR0 after writing, store at 0xC4 */
	mov	x3, #0xC4
	ldr	w4, [x2]		// x2 = GIC_DISTB + IGROUPR (first reg)
	str	w4, [x3]

	/* Read back GICD_CTLR, store at 0xCC */
	sub	x5, x2, #GICD_IGROUPR	// x5 = GIC_DISTB
	ldr	w4, [x5, #GICD_CTRLR]
	mov	x3, #0xCC
	str	w4, [x3]

	ret

.globl dtb_space
dtb_space:

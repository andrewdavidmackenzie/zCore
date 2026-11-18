# Context switch
#
#   void swtch(struct ContextData *old, struct ContextData *new);
#
# Save current registers in old. Load from new.
# Saves: x19-x29, lr, sp, ttbr0, q0-q31, fpcr, fpsr

    .text
    .globl switch
    .balign 4
switch:
        # Save GP registers (x19-x29, lr, sp, ttbr0) at offsets 0-111
        add     x0, x0, #112
        mrs     x2, ttbr0_el1
        mov     x3, sp
        stp     x3, x2, [x0, #-16]!
        stp     x29, x30, [x0, #-16]!
        stp     x27, x28, [x0, #-16]!
        stp     x25, x26, [x0, #-16]!
        stp     x23, x24, [x0, #-16]!
        stp     x21, x22, [x0, #-16]!
        stp     x19, x20, [x0, #-16]!

        # Save FPU/SIMD registers (q0-q31) at offset 112..624
        # x0 is now at the start of the struct; FPU starts at +112
        add     x0, x0, #112
        stp     q0,  q1,  [x0, #0]
        stp     q2,  q3,  [x0, #32]
        stp     q4,  q5,  [x0, #64]
        stp     q6,  q7,  [x0, #96]
        stp     q8,  q9,  [x0, #128]
        stp     q10, q11, [x0, #160]
        stp     q12, q13, [x0, #192]
        stp     q14, q15, [x0, #224]
        stp     q16, q17, [x0, #256]
        stp     q18, q19, [x0, #288]
        stp     q20, q21, [x0, #320]
        stp     q22, q23, [x0, #352]
        stp     q24, q25, [x0, #384]
        stp     q26, q27, [x0, #416]
        stp     q28, q29, [x0, #448]
        stp     q30, q31, [x0, #480]
        # Save FPCR and FPSR at offset 624 (512 from fpu_regs base)
        mrs     x2, fpcr
        mrs     x3, fpsr
        add     x4, x0, #512
        str     w2, [x4]
        str     w3, [x4, #4]

        # --- Restore from new context (x1) ---

        # Restore TTBR0 and flush TLB if changed
        ldr     x9, [x1, #104]
        cbz     x9, 1f
        lsr     x10, x9, #12
        msr     ttbr0_el1, x9
        dsb     ishst
        tlbi    vaae1is, x10
        dsb     ish
        isb
1:
        # Restore GP registers
        ldp     x19, x20, [x1], #16
        ldp     x21, x22, [x1], #16
        ldp     x23, x24, [x1], #16
        ldp     x25, x26, [x1], #16
        ldp     x27, x28, [x1], #16
        ldp     x29, x30, [x1], #16
        ldr     x0, [x1], #8
        mov     sp, x0

        # x1 is at offset 104 (after sp load); skip ttbr0 to reach fpu_regs at 112
        add     x1, x1, #8
        # Restore FPU/SIMD registers (q0-q31)
        ldp     q0,  q1,  [x1, #0]
        ldp     q2,  q3,  [x1, #32]
        ldp     q4,  q5,  [x1, #64]
        ldp     q6,  q7,  [x1, #96]
        ldp     q8,  q9,  [x1, #128]
        ldp     q10, q11, [x1, #160]
        ldp     q12, q13, [x1, #192]
        ldp     q14, q15, [x1, #224]
        ldp     q16, q17, [x1, #256]
        ldp     q18, q19, [x1, #288]
        ldp     q20, q21, [x1, #320]
        ldp     q22, q23, [x1, #352]
        ldp     q24, q25, [x1, #384]
        ldp     q26, q27, [x1, #416]
        ldp     q28, q29, [x1, #448]
        ldp     q30, q31, [x1, #480]
        # Restore FPCR and FPSR
        add     x2, x1, #512
        ldr     w3, [x2]
        ldr     w4, [x2, #4]
        msr     fpcr, x3
        msr     fpsr, x4

        ret

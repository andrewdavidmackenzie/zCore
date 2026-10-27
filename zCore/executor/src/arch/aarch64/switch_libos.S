// Context switch (libos mode — no TTBR0_EL1 manipulation)
//
//   void swtch(struct ContextData *old, struct ContextData *new);
//
// Save current registers in old. Load from new.
// Define both _switch (macOS) and switch (Linux) as global symbols.

    .text
    .globl _switch
    .globl switch
    .balign 4
_switch:
switch:
        add     x0, x0, #112
        mov     x2, #0          // dummy TTBR0 (was: mrs x2, ttbr0_el1)
        mov     x3, sp
        stp     x3, x2, [x0, #-16]!
        stp     x29, x30, [x0, #-16]!
        stp     x27, x28, [x0, #-16]!
        stp     x25, x26, [x0, #-16]!
        stp     x23, x24, [x0, #-16]!
        stp     x21, x22, [x0, #-16]!
        stp     x19, x20, [x0, #-16]!

        // Skip TTBR0 restore
        ldp     x19, x20, [x1], #16
        ldp     x21, x22, [x1], #16
        ldp     x23, x24, [x1], #16
        ldp     x25, x26, [x1], #16
        ldp     x27, x28, [x1], #16
        ldp     x29, x30, [x1], #16
        ldr     x0, [x1], #8
        mov     sp, x0
        ret

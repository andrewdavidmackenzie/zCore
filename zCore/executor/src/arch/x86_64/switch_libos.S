# Context switch (libos mode — no CR3 manipulation)
#
#   void swtch(struct ContextData *old, struct ContextData *new);
#
# Save current registers in old. Load from new.
# Same as bare-metal switch.S but without CR3 save/restore,
# which is a privileged operation not available in user mode.

    .text
    .globl switch
    .balign 4
switch:
        // push rip by caller

        // Save callee-save registers
        push rbx
        push rbp
        push r12
        push r13
        push r14
        push r15
        // Skip CR3 — push a dummy value to keep stack layout compatible
        push 0

        mov [rdi], rsp      // rdi = from_rsp
        mov rsp, [rsi]      // rsi = to_rsp

        // Pop callee-save registers
        // Skip CR3 dummy
        add rsp, 8
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        // pop rip
        ret

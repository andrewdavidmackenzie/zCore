# Context switch
#
#   void swtch(struct ContextData *old, struct ContextData *new);
#
# Save current registers in old. Load from new.
#
# CR3 is saved/restored to maintain the kernel page table across
# executor context switches. ThreadSwitchFuture::poll handles the
# user ↔ kernel page table transitions with active_cpus tracking.
# The CR3 saved here should always be the kernel page table (since
# switch_to_kernel_page_table is called after each user poll).
#
# RFLAGS is saved/restored so the resumed context keeps its own
# interrupt state. run_until_idle switches with IF=0 (its runtime-lock
# discipline); without restoring rflags, every task resumed from an
# await park inherited IF=0 and ran kernel code with interrupts
# disabled — no timer, and contended KernelMutexes hit the #862
# nested-spin panic (the crash-cascade multi-CPU panics).

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
        mov r15, cr3
        push r15
        pushfq

        mov [rdi], rsp      // rdi = from_rsp
        mov rsp, [rsi]      // rsi = to_rsp

        // Restore interrupt state first (see above)
        popfq
        // Pop callee-save registers
        pop r15
        mov cr3, r15
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        // pop rip
        ret

# Context switch without saving the current context.
#
#   void switch_no_save(struct ContextData *new);
#
# Loads the context at `new` and resumes it; the current context is
# discarded. Used by the kernel-trap fatal path
# (executor::abandon_current_poll): a faulted poll must never be
# resumed, so its context is simply not saved.
    .text
    .globl switch_no_save
    .balign 4
switch_no_save:
        // One argument: `new` arrives in rdi per the System V ABI
        // (not rsi — that is the second argument of the two-arg
        // `switch` above). Reading [rsi] here only worked when the
        // caller's register allocation happened to leave `new` in
        // rsi as well.
        mov rsp, [rdi]      // rdi = to_rsp

        // Restore interrupt state first (see above)
        popfq
        // Pop callee-save registers
        pop r15
        mov cr3, r15
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        // pop rip
        ret

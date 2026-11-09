# MSR numbers for FS/GS base access (fallback when FSGSBASE not available)
.set MSR_FS_BASE,           0xC0000100
.set MSR_GS_BASE,           0xC0000101
.set MSR_KERNEL_GS_BASE,    0xC0000102

.text
    # extern "sysv64" fn syscall_return(&mut GeneralRegs)
.global syscall_return
syscall_return:
    # disable interrupt
    cli

    # save callee-saved registers
    push r15
    push r14
    push r13
    push r12
    push rbp
    push rbx

    # save fsbase, kernel may use
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    rdfsbase rbx
    jmp 2f
1:  # MSR fallback: read IA32_FS_BASE
    mov ecx, MSR_FS_BASE
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov rbx, rax
2:
    push rbx

    push rdi
    push rdi                # keep rsp 16 bytes align
    mov gs:4, rsp           # store kernel rsp -> TSS.sp0
    mov rsp, rdi            # set rsp = bottom of trap frame

    # pop fsbase gsbase
    swapgs                  # store kernel gsbase

    # restore user fsbase
    mov rax, [rsp + 18*8]
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    wrfsbase rax
    jmp 2f
1:  # MSR fallback: write IA32_FS_BASE
    mov rdx, rax
    shr rdx, 32
    mov ecx, MSR_FS_BASE
    wrmsr
2:

    # restore user gsbase
    mov rax, [rsp + 19*8]
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    wrgsbase rax
    jmp 2f
1:  # MSR fallback: write IA32_GS_BASE
    # After swapgs above, kernel GS is active. User GS value goes
    # into IA32_KERNEL_GS_BASE (swapgs will swap it back on sysret/iret).
    # But the original code used wrgsbase which writes the current GS base
    # directly. After swapgs, current GS = kernel GS, so wrgsbase would
    # overwrite the kernel GS base. The original code relies on swapgs
    # happening again before returning to user. Let's match the original
    # semantics: write to the CURRENT GS base register.
    mov rdx, rax
    shr rdx, 32
    mov ecx, MSR_GS_BASE
    wrmsr
2:

    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8                  # skip rsp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    # rip
    # rflags
    # fsbase
    # gsbase
    # trap_num
    # error_code

    # determain sysret or iret
    cmp dword ptr [rsp + 4*8], 0x100  # syscall?
    je sysret
iret:
    # construct trap frame
    push [USER_SS]          # push ss
    push [rsp - 8*8]        # push rsp
    push [rsp + 3*8]        # push rflags
    push [USER_CS]          # push cs
    push [rsp + 4*8]        # push rip

    iretq

sysret:
    pop rcx                 # rcx = rip
    pop r11                 # r11 = rflags
    mov rsp, [rsp - 11*8]   # load rsp

    sysretq

    # sysretq instruction do:
    # - load cs, ss
    # - load rflags <- r11
    # - load rip <- rcx


.global syscall_entry
syscall_entry:
    # syscall instruction do:
    # - load cs
    # - store rflags -> r11
    # - mask rflags
    # - store rip -> rcx
    # - load rip

    swapgs                  # swap in kernel gs
    mov gs:12, rsp          # store user rsp -> scratch at TSS.sp1
    mov rsp, gs:4           # load kernel rsp <- TSS.sp0
    pop rsp                 # load rsp = bottom of trap frame
    add rsp, 22*8           # rsp = top of trap frame

    # push trap_num, error_code
    push 0                  # push error_code
    push 0x100              # push trap_num
    sub rsp, 16             # skip fsbase, gsbase
    # push general registers
    push r11                # push rflags
    push rcx                # push rip

.global trap_syscall_entry
trap_syscall_entry:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push gs:12              # push rsp
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax

    # save user fsbase
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    rdfsbase rbx
    jmp 2f
1:  # MSR fallback: read IA32_FS_BASE
    mov ecx, MSR_FS_BASE
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov rbx, rax
2:
    mov [rsp + 18*8], rbx

    # save user gsbase (need swapgs dance to read user GS)
    swapgs
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    rdgsbase rbx
    jmp 2f
1:  # MSR fallback: read current GS base (which is user GS after swapgs)
    mov ecx, MSR_GS_BASE
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov rbx, rax
2:
    mov [rsp + 19*8], rbx
    swapgs

    # restore callee-saved registers
    mov rsp, gs:4           # load kernel rsp <- TSS.sp0
    pop rbx
    pop rbx

    # restore fsbase of kernel
    pop rbx
    cmp byte ptr [rip + TRAPFRAME_FSGSBASE], 0
    je 1f
    wrfsbase rbx
    jmp 2f
1:  # MSR fallback: write IA32_FS_BASE
    mov rax, rbx
    mov rdx, rbx
    shr rdx, 32
    mov ecx, MSR_FS_BASE
    wrmsr
2:

    pop rbx
    pop rbp
    pop r12
    pop r13
    pop r14
    pop r15

    # go back to Rust
    ret

; AP Trampoline
; TrampolineData at 0x8100 (repr(C) with padding):
;   +0x00: cr3 (u64)
;   +0x08: entry (u64)
;   +0x10: stack_top (u64)
;   +0x18: gdt_ptr (packed: u16 limit + u64 base = 10 bytes)
;   +0x22: 2 bytes padding (AtomicU32 alignment)
;   +0x24: ap_ready (u32)
;   Total size: 40 bytes (0x28)
; GDT at 0x8128 (0x100 + 40 = 0x128)

BITS 16
ORG 0x8000

start:
    cli
    cld
    xor ax, ax
    mov ds, ax

    ; Load GDT (o32 for 32-bit base)
    o32 lgdt [0x8118]       ; gdt_ptr at TrampolineData + 0x18

    ; Enable protected mode
    mov eax, cr0
    or al, 1
    mov cr0, eax

    ; Far jump to 32-bit protected mode (selector 0x08)
    jmp dword 0x08:pm32

BITS 32
pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Enable PAE
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    ; Load CR3 from trampoline data
    mov eax, [0x8100]       ; cr3 at +0x00
    mov cr3, eax

    ; Enable long mode via IA32_EFER MSR
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    ; Enable paging
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    ; Far jump to 64-bit code (selector 0x18)
    jmp dword 0x18:lm64

BITS 64
lm64:
    ; Load stack
    mov rsp, [0x8110]       ; stack_top at +0x10
    ; Load entry point
    mov rax, [0x8108]       ; entry at +0x08

    ; Signal BSP: write APIC ID to ap_ready
    push rax
    mov eax, 1
    cpuid
    shr ebx, 24
    mov [0x8122], ebx       ; ap_ready at +0x22
    pop rax

    ; Jump to Rust entry
    jmp rax

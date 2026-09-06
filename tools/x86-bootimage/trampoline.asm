; AP Trampoline: 16-bit real mode -> 32-bit protected mode -> 64-bit long mode
; Assembled at org 0x8000 (copied there at runtime)
; TrampolineData is at 0x8100 (TRAMPOLINE_DATA_OFFSET = 0x100):
;   +0x00: cr3 (u64)
;   +0x08: entry (u64)
;   +0x10: stack_top (u64)
;   +0x1a: gdt_ptr (2 bytes limit + 8 bytes base, but lgdt reads 2+3 or 2+4)
;   +0x22: ap_ready (u32)
; GDT is at 0x8126 (gdt_offset = 0x100 + 38 = 0x126)

BITS 16
ORG 0x8000

start:
    cli
    cld
    xor ax, ax
    mov ds, ax

    ; Load GDT (need 32-bit base, use o32 override)
    o32 lgdt [0x811a]

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
    mov eax, [0x8100]
    mov cr3, eax

    ; Enable long mode via IA32_EFER MSR
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    ; Enable paging (activates long mode)
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    ; Far jump to 64-bit code (selector 0x18)
    jmp dword 0x18:lm64

BITS 64
lm64:
    ; Load stack from trampoline data
    mov rsp, [0x8110]
    ; Load entry point
    mov rax, [0x8108]

    ; Signal BSP: write APIC ID to ap_ready
    push rax
    mov eax, 1
    cpuid
    shr ebx, 24
    mov [0x8122], ebx
    pop rax

    ; Jump to Rust entry
    jmp rax

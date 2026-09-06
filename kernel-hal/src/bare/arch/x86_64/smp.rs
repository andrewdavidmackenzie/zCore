//! x86_64 SMP (Symmetric Multi-Processing) boot support.
//!
//! Wakes Application Processors (APs) using the INIT-SIPI-SIPI sequence.
//! Each AP starts in 16-bit real mode at a trampoline page, transitions
//! through protected mode to long mode, then jumps to the kernel's
//! `secondary_main()` function.

use crate::{mem::phys_to_virt, KCONFIG};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Physical address where the AP trampoline code is copied.
/// Must be page-aligned and below 1MB (SIPI vector = phys_addr >> 12).
const TRAMPOLINE_PHYS: usize = 0x8000;

/// SIPI vector: trampoline physical address >> 12.
const SIPI_VECTOR: u8 = (TRAMPOLINE_PHYS >> 12) as u8;

/// Size of per-AP kernel stack (128 KiB).
const AP_STACK_SIZE: usize = 128 * 1024;

/// Maximum number of APs supported.
const MAX_APS: usize = 7; // 8 cores total, 1 BSP

/// Shared data between BSP and AP trampoline, placed right after the
/// trampoline code at a known offset. The trampoline assembly reads
/// these fields to set up CR3, stack, and jump target.
///
/// This struct is laid out at `TRAMPOLINE_PHYS + TRAMPOLINE_DATA_OFFSET`.
#[repr(C)]
struct TrampolineData {
    /// CR3 value (physical address of PML4 page table).
    cr3: u64,
    /// Virtual address of the Rust entry point for the AP.
    entry: u64,
    /// Virtual address of the top of the AP's kernel stack.
    stack_top: u64,
    /// GDT pointer (limit + base) for the AP to load.
    gdt_ptr: GdtPtr,
    /// Atomic flag: AP sets this to its APIC ID when it's alive.
    ap_ready: AtomicU32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct GdtPtr {
    limit: u16,
    base: u64,
}

// Offset of TrampolineData within the trampoline page.
// The trampoline code occupies the first part; data starts here.
const TRAMPOLINE_DATA_OFFSET: usize = 0x100;

/// The AP trampoline code (16-bit real mode -> 64-bit long mode).
///
/// This is position-dependent code that must run at TRAMPOLINE_PHYS.
/// It reads TrampolineData at TRAMPOLINE_PHYS + TRAMPOLINE_DATA_OFFSET
/// for CR3, stack, and entry point.
///
/// Layout:
/// - Starts in 16-bit real mode (CS:IP = 0:TRAMPOLINE_PHYS)
/// - Loads a temporary GDT with 32-bit and 64-bit code segments
/// - Enables protected mode (CR0.PE)
/// - Jumps to 32-bit code
/// - Enables PAE (CR4.PAE)
/// - Loads CR3 from TrampolineData
/// - Enables long mode (EFER.LME)
/// - Enables paging (CR0.PG)
/// - Jumps to 64-bit code
/// - Loads stack and entry point from TrampolineData
/// - Calls the Rust entry point
const TRAMPOLINE_CODE: &[u8] = &{
    // We use a const byte array because global_asm! with .code16 is
    // fragile across toolchains. This is the assembled output of:
    //
    // .code16
    // .org 0x8000
    // start:
    //   cli
    //   xor ax, ax
    //   mov ds, ax
    //   ; Load GDT from TrampolineData
    //   lgdt [0x8000 + TRAMPOLINE_DATA_OFFSET + offset_of(gdt_ptr)]
    //   ; Enable protected mode
    //   mov eax, cr0
    //   or al, 1
    //   mov cr0, eax
    //   ; Far jump to 32-bit protected mode
    //   jmp 0x08:pm_entry   ; (0x08 = first GDT code segment)
    //
    // .code32
    // pm_entry:
    //   mov ax, 0x10        ; data segment selector
    //   mov ds, ax
    //   mov es, ax
    //   mov ss, ax
    //   ; Enable PAE
    //   mov eax, cr4
    //   or eax, 0x20        ; CR4.PAE
    //   mov cr4, eax
    //   ; Load CR3 from TrampolineData
    //   mov eax, [0x8000 + TRAMPOLINE_DATA_OFFSET + 0]  ; cr3 field
    //   mov cr3, eax
    //   ; Enable long mode via EFER MSR
    //   mov ecx, 0xC0000080 ; IA32_EFER
    //   rdmsr
    //   or eax, 0x100       ; EFER.LME
    //   wrmsr
    //   ; Enable paging
    //   mov eax, cr0
    //   or eax, 0x80000000  ; CR0.PG
    //   mov cr0, eax
    //   ; Far jump to 64-bit long mode
    //   jmp 0x18:lm_entry   ; (0x18 = 64-bit code segment in GDT)
    //
    // .code64
    // lm_entry:
    //   ; Load stack from TrampolineData
    //   mov rsp, [0x8000 + TRAMPOLINE_DATA_OFFSET + 16]  ; stack_top
    //   ; Load entry point
    //   mov rax, [0x8000 + TRAMPOLINE_DATA_OFFSET + 8]   ; entry
    //   ; Signal BSP that we're alive (write APIC ID to ap_ready)
    //   ; APIC ID is in CPUID.01H:EBX[31:24]
    //   push rax
    //   mov eax, 1
    //   cpuid
    //   shr ebx, 24
    //   mov [0x8000 + TRAMPOLINE_DATA_OFFSET + 34], ebx  ; ap_ready
    //   pop rax
    //   ; Jump to Rust entry point
    //   jmp rax
    //
    // The GDT embedded in TrampolineData has:
    //   0x00: null descriptor
    //   0x08: 32-bit code segment (CS for protected mode)
    //   0x10: 32-bit data segment
    //   0x18: 64-bit code segment (CS for long mode)

    // NOTE: Rather than embedding raw bytes (fragile), we'll use
    // global_asm! with proper .code16/.code32/.code64 directives.
    // This const is a placeholder -- the actual code is in ap_trampoline.S
    *b""
};

// Use global_asm! for the trampoline instead of raw bytes.
// The trampoline is assembled as a separate section that we copy at runtime.
core::arch::global_asm!(
    r#"
.section .rodata
.global ap_trampoline_start
.global ap_trampoline_end

ap_trampoline_start:

.code16
    cli
    cld
    xor ax, ax
    mov ds, ax

    // Load GDT pointer from data area
    lgdt [{trampoline_phys} + {data_offset} + 26]

    // Enable protected mode
    mov eax, cr0
    or al, 1
    mov cr0, eax

    // Far jump to 32-bit protected mode code
    .byte 0x66, 0xea             // ljmp (32-bit operand override)
    .long {trampoline_phys} + (.Lpm32 - ap_trampoline_start)
    .word 0x08                   // code32 selector

.code32
.Lpm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    // Enable PAE (required for long mode)
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    // Load CR3 (page table root) from trampoline data
    mov eax, [{trampoline_phys} + {data_offset}]
    mov cr3, eax

    // Enable long mode via IA32_EFER MSR
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)            // LME (Long Mode Enable)
    wrmsr

    // Enable paging (activates long mode with PAE+LME)
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    // Far jump to 64-bit long mode
    .byte 0xea                   // ljmp
    .long {trampoline_phys} + (.Llm64 - ap_trampoline_start)
    .word 0x18                   // code64 selector

.code64
.Llm64:
    // Load kernel stack from trampoline data
    mov rsp, [{trampoline_phys} + {data_offset} + 16]

    // Load entry point from trampoline data
    mov rax, [{trampoline_phys} + {data_offset} + 8]

    // Write APIC ID to ap_ready to signal BSP
    push rax
    mov eax, 1
    cpuid
    shr ebx, 24
    mov [{trampoline_phys} + {data_offset} + 34], ebx
    pop rax

    // Jump to Rust entry point (secondary_main via ap_entry)
    jmp rax

.balign 4
ap_trampoline_end:

// Restore default code size for the rest of the kernel
.code64
"#,
    trampoline_phys = const TRAMPOLINE_PHYS,
    data_offset = const TRAMPOLINE_DATA_OFFSET,
);

extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

/// Temporary GDT for the AP trampoline (null + code32 + data32 + code64).
static TRAMPOLINE_GDT: [u64; 4] = [
    0x0000_0000_0000_0000, // 0x00: null
    0x00CF_9A00_0000_FFFF, // 0x08: code32 (exec/read, 32-bit, 4G limit)
    0x00CF_9200_0000_FFFF, // 0x10: data32 (read/write, 32-bit, 4G limit)
    0x0020_9800_0000_0000, // 0x18: code64 (exec/read, long mode)
];

/// Rust entry point for APs after the trampoline completes.
/// Sets up FSGSBASE, calls trapframe::init(), then jumps to secondary_main.
extern "C" fn ap_entry() -> ! {
    // Enable FSGSBASE (required by trapframe)
    unsafe {
        use x86_64::registers::control::{Cr4, Cr4Flags};
        Cr4::update(|f| {
            f.insert(Cr4Flags::FSGSBASE);
            f.insert(Cr4Flags::OSFXSR);
            f.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
    // Call the kernel's secondary_main (stored in KernelConfig)
    (KCONFIG.ap_fn)()
}

/// Boot all Application Processors.
///
/// Called from `primary_init()` after the BSP has finished initialization.
/// Enumerates CPUs via ACPI MADT, allocates per-AP stacks, copies the
/// trampoline to low memory, and sends INIT-SIPI-SIPI to each AP.
pub fn boot_application_processors() {
    let rsdp = KCONFIG.acpi_rsdp;
    if rsdp == 0 {
        warn!("No ACPI RSDP -- cannot enumerate APs, skipping SMP boot");
        return;
    }

    // Enumerate APs via ACPI
    let ap_ids = enumerate_aps(rsdp as usize);
    if ap_ids.is_empty() {
        info!("No application processors found");
        return;
    }
    info!(
        "Found {} application processor(s): {:?}",
        ap_ids.len(),
        ap_ids
    );

    // Copy trampoline code to low physical memory
    let trampoline_size = core::ptr::addr_of!(ap_trampoline_end) as usize
        - core::ptr::addr_of!(ap_trampoline_start) as usize;
    assert!(
        trampoline_size + TRAMPOLINE_DATA_OFFSET <= 0x1000,
        "trampoline too large for one page"
    );

    let trampoline_virt = phys_to_virt(TRAMPOLINE_PHYS);
    let trampoline_src = core::ptr::addr_of!(ap_trampoline_start) as *const u8;
    info!(
        "Copying {} bytes of trampoline from {:#x} to virt {:#x} (phys {:#x})",
        trampoline_size, trampoline_src as usize, trampoline_virt, TRAMPOLINE_PHYS,
    );
    unsafe {
        core::ptr::copy_nonoverlapping(trampoline_src, trampoline_virt as *mut u8, trampoline_size);
    }
    // Verify the copy worked
    let first_bytes = unsafe { core::slice::from_raw_parts(trampoline_virt as *const u8, 4) };
    info!("Trampoline first 4 bytes at dest: {:02x?}", first_bytes);

    // Get current CR3 (kernel page table)
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
    }

    // Boot each AP one at a time
    for &apic_id in &ap_ids {
        // Allocate a stack for this AP
        let stack = unsafe {
            alloc::alloc::alloc(alloc::alloc::Layout::from_size_align(AP_STACK_SIZE, 16).unwrap())
        };
        if stack.is_null() {
            error!("Failed to allocate stack for AP {}", apic_id);
            continue;
        }
        let stack_top = stack as u64 + AP_STACK_SIZE as u64;

        // Set up trampoline data
        let data_virt = phys_to_virt(TRAMPOLINE_PHYS + TRAMPOLINE_DATA_OFFSET);
        let data = unsafe { &mut *(data_virt as *mut TrampolineData) };
        data.cr3 = cr3;
        data.entry = ap_entry as *const () as u64;
        data.stack_top = stack_top;
        data.gdt_ptr = GdtPtr {
            limit: (core::mem::size_of_val(&TRAMPOLINE_GDT) - 1) as u16,
            base: TRAMPOLINE_PHYS as u64 + gdt_offset() as u64,
        };
        data.ap_ready.store(0, Ordering::SeqCst);

        // Copy the GDT into the trampoline page (must be accessible in real/protected mode)
        let gdt_dest = phys_to_virt(TRAMPOLINE_PHYS + gdt_offset());
        unsafe {
            core::ptr::copy_nonoverlapping(
                TRAMPOLINE_GDT.as_ptr() as *const u8,
                gdt_dest as *mut u8,
                core::mem::size_of_val(&TRAMPOLINE_GDT),
            );
        }

        // Send INIT-SIPI-SIPI sequence
        info!("Starting AP {} ...", apic_id);
        let lapic = zcore_drivers::irq::x86::Apic::local_apic();

        // INIT IPI
        lapic.send_init_ipi(apic_id);
        // Wait 10ms
        spin_delay_ms(10);

        // SIPI (first)
        lapic.send_sipi(SIPI_VECTOR, apic_id);
        // Wait 200us
        spin_delay_us(200);

        // SIPI (second, per Intel MP spec)
        if data.ap_ready.load(Ordering::SeqCst) == 0 {
            lapic.send_sipi(SIPI_VECTOR, apic_id);
            spin_delay_us(200);
        }

        // Wait for AP to signal it's alive (up to 100ms)
        let mut waited = 0u32;
        while data.ap_ready.load(Ordering::SeqCst) == 0 && waited < 100_000 {
            spin_delay_us(100);
            waited += 100;
        }

        if data.ap_ready.load(Ordering::SeqCst) != 0 {
            info!(
                "AP {} started (APIC ID {})",
                apic_id,
                data.ap_ready.load(Ordering::SeqCst)
            );
        } else {
            warn!("AP {} did not respond to SIPI", apic_id);
        }
    }
}

/// Offset of the GDT within the trampoline page (after TrampolineData).
const fn gdt_offset() -> usize {
    // Place GDT right after TrampolineData
    TRAMPOLINE_DATA_OFFSET + core::mem::size_of::<TrampolineData>()
}

/// ACPI handler that maps physical addresses using the kernel's phys_to_virt.
#[derive(Clone)]
struct SmpAcpiHandler;

impl acpi::AcpiHandler for SmpAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let page_size = 4096usize;
        let aligned_start = physical_address & !(page_size - 1);
        let aligned_end = (physical_address + size + page_size - 1) & !(page_size - 1);
        acpi::PhysicalMapping::new(
            physical_address,
            core::ptr::NonNull::new_unchecked(phys_to_virt(physical_address) as *mut T),
            size,
            aligned_end - aligned_start,
            self.clone(),
        )
    }
    fn unmap_physical_region<T>(_region: &acpi::PhysicalMapping<Self, T>) {}
}

/// Enumerate AP APIC IDs from ACPI MADT.
fn enumerate_aps(rsdp: usize) -> Vec<u32> {
    use acpi::platform::ProcessorState;

    let handler = SmpAcpiHandler;
    let tables = match unsafe { acpi::AcpiTables::from_rsdp(handler, rsdp) } {
        Ok(t) => t,
        Err(e) => {
            warn!("Failed to parse ACPI tables: {:?}", e);
            return Vec::new();
        }
    };

    let platform_info = match tables.platform_info() {
        Ok(info) => info,
        Err(e) => {
            warn!("Failed to get platform info: {:?}", e);
            return Vec::new();
        }
    };

    let proc_info = match &platform_info.processor_info {
        Some(info) => info,
        None => {
            warn!("No processor info in ACPI tables");
            return Vec::new();
        }
    };

    proc_info
        .application_processors
        .iter()
        .filter(|p| p.state == ProcessorState::WaitingForSipi)
        .map(|p| p.local_apic_id)
        .take(MAX_APS)
        .collect()
}

/// Spin-wait delay in milliseconds (approximate, uses TSC).
fn spin_delay_ms(ms: u32) {
    spin_delay_us(ms * 1000);
}

/// Spin-wait delay in microseconds (approximate, uses TSC).
fn spin_delay_us(us: u32) {
    let freq_mhz = super::cpu::cpu_frequency() as u64;
    let cycles = freq_mhz * us as u64;
    let start = unsafe { core::arch::x86_64::_rdtsc() };
    while unsafe { core::arch::x86_64::_rdtsc() } - start < cycles {
        core::hint::spin_loop();
    }
}

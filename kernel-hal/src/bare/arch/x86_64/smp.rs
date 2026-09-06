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
/// Pre-assembled AP trampoline binary (NASM output).
///
/// Assembled with: nasm -f bin trampoline.asm
/// Source is in the doc comment of `boot_application_processors()`.
///
/// This is a flat binary that runs at physical address 0x8000.
/// It transitions from 16-bit real mode through 32-bit protected mode
/// to 64-bit long mode, then jumps to the Rust `ap_entry()` function.
///
/// Hard-coded addresses within the binary:
/// - LGDT loads from 0x811a (TrampolineData.gdt_ptr)
/// - CR3 loaded from 0x8100 (TrampolineData.cr3)
/// - Entry point loaded from 0x8108 (TrampolineData.entry)
/// - Stack loaded from 0x8110 (TrampolineData.stack_top)
/// - APIC ID written to 0x8122 (TrampolineData.ap_ready)
#[rustfmt::skip]
const AP_TRAMPOLINE: [u8; 148] = [
    // 16-bit real mode: cli, cld, xor ax,ax, mov ds,ax, serial 'A'
    0xfa, 0xfc, 0x31, 0xc0, 0x8e, 0xd8, 0xb0, 0x41, 0xba, 0xf8, 0x03, 0xee,
    // lgdt [0x8118], enable PE, ljmp 0x08:0x8022
    0x66, 0x0f, 0x01, 0x16, 0x18, 0x81, 0x0f, 0x20, 0xc0, 0x0c, 0x01, 0x0f,
    0x22, 0xc0, 0x66, 0xea, 0x22, 0x80, 0x00, 0x00, 0x08, 0x00,
    // 32-bit: serial 'B', load segments, PAE, CR3, EFER(LME+NXE), PG
    0xb0, 0x42, 0x66, 0xba, 0xf8, 0x03, 0xee,
    0x66, 0xb8, 0x10, 0x00, 0x8e, 0xd8, 0x8e, 0xc0,
    0x8e, 0xe0, 0x8e, 0xe8, 0x8e, 0xd0,
    0x0f, 0x20, 0xe0, 0x83, 0xc8, 0x20, 0x0f, 0x22, 0xe0,
    0xa1, 0x00, 0x81, 0x00, 0x00, 0x0f, 0x22, 0xd8,
    0xb9, 0x80, 0x00, 0x00, 0xc0, 0x0f, 0x32,
    0x0d, 0x00, 0x09, 0x00, 0x00, 0x0f, 0x30,
    0x0f, 0x20, 0xc0, 0x0d, 0x00, 0x00, 0x00, 0x80, 0x0f, 0x22, 0xc0,
    // ljmp 0x18:0x8068
    0xea, 0x68, 0x80, 0x00, 0x00, 0x18, 0x00,
    // 64-bit: serial 'C', load stack+entry, signal BSP, jmp entry
    0xb0, 0x43, 0x66, 0xba, 0xf8, 0x03, 0xee,
    0x48, 0x8b, 0x24, 0x25, 0x10, 0x81, 0x00, 0x00, // mov rsp,[0x8110]
    0x48, 0x8b, 0x04, 0x25, 0x08, 0x81, 0x00, 0x00, // mov rax,[0x8108]
    0x50,                                             // push rax
    0xb8, 0x01, 0x00, 0x00, 0x00, 0x0f, 0xa2,         // cpuid(1)
    0xc1, 0xeb, 0x18,                                 // shr ebx,24
    0x89, 0x1c, 0x25, 0x24, 0x81, 0x00, 0x00,         // mov [0x8124],ebx (ap_ready @ +36)
    0x58,                                             // pop rax
    0xff, 0xe0,                                       // jmp rax
];

/// Temporary GDT for the AP trampoline (null + code32 + data32 + code64).
static TRAMPOLINE_GDT: [u64; 4] = [
    0x0000_0000_0000_0000, // 0x00: null
    0x00CF_9A00_0000_FFFF, // 0x08: code32 (exec/read, 32-bit, 4G limit)
    0x00CF_9200_0000_FFFF, // 0x10: data32 (read/write, 32-bit, 4G limit)
    0x0020_9800_0000_0000, // 0x18: code64 (exec/read, long mode)
];

/// Rust entry point for APs after the trampoline completes.
/// Sets up CPU features and calls secondary_main.
extern "C" fn ap_entry() -> ! {
    unsafe {
        // Enable NXE in EFER -- the kernel page table uses NX bits on data
        // pages. Without NXE, bit 63 in PTEs is reserved, causing #PF.
        use x86_64::registers::model_specific::{Efer, EferFlags};
        Efer::update(|f| f.insert(EferFlags::NO_EXECUTE_ENABLE));

        // Enable FSGSBASE (required by trapframe) and SSE
        use x86_64::registers::control::{Cr4, Cr4Flags};
        Cr4::update(|f| {
            f.insert(Cr4Flags::FSGSBASE);
            f.insert(Cr4Flags::OSFXSR);
            f.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });

        // Clear CR0.EM for SSE
        use x86_64::registers::control::{Cr0, Cr0Flags};
        Cr0::update(|f| f.remove(Cr0Flags::EMULATE_COPROCESSOR));
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

    // Copy pre-assembled trampoline binary to low physical memory
    assert!(
        AP_TRAMPOLINE.len() + TRAMPOLINE_DATA_OFFSET <= 0x1000,
        "trampoline too large for one page"
    );

    let trampoline_virt = phys_to_virt(TRAMPOLINE_PHYS);
    info!(
        "Copying {} bytes of trampoline to phys {:#x}",
        AP_TRAMPOLINE.len(),
        TRAMPOLINE_PHYS,
    );
    unsafe {
        core::ptr::copy_nonoverlapping(
            AP_TRAMPOLINE.as_ptr(),
            trampoline_virt as *mut u8,
            AP_TRAMPOLINE.len(),
        );
    }

    // Get current CR3 (kernel page table)
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
    }

    // Add a temporary identity mapping for the trampoline page.
    // When the AP enables paging in protected mode, it's still executing
    // at physical address 0x8000. Without an identity mapping, the
    // instruction fetch immediately after CR0.PG=1 will fault.
    add_identity_mapping(cr3 as usize, TRAMPOLINE_PHYS);

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
        // Use broadcast IPIs (all-excluding-self) for simplicity.
        // This works when booting APs one at a time because we only
        // send SIPI after INIT, and only APs in SIPI-wait state respond.
        let lapic = zcore_drivers::irq::x86::Apic::local_apic();

        let dbg_stack = data.stack_top;
        let dbg_entry = data.entry;
        let dbg_cr3 = data.cr3;
        let dbg_gdt = { data.gdt_ptr.base };
        // Verify via identity-mapped read
        let verify_entry = unsafe {
            core::ptr::read_volatile(
                (phys_to_virt(TRAMPOLINE_PHYS + TRAMPOLINE_DATA_OFFSET + 8)) as *const u64,
            )
        };
        info!(
            "AP {} config: stack_top={:#x}, entry={:#x} (verify={:#x}), cr3={:#x}, gdt_base={:#x}",
            apic_id, dbg_stack, dbg_entry, verify_entry, dbg_cr3, dbg_gdt
        );

        // Send INIT-SIPI-SIPI without holding any locks.
        lapic.send_init_ipi_all();
        // 10ms delay (simple loop -- TSC may not work reliably during AP startup)
        for _ in 0..10_000_000u64 {
            core::hint::spin_loop();
        }

        lapic.send_sipi_all(SIPI_VECTOR);
        for _ in 0..200_000u64 {
            core::hint::spin_loop();
        }

        if data.ap_ready.load(Ordering::SeqCst) == 0 {
            lapic.send_sipi_all(SIPI_VECTOR);
            for _ in 0..200_000u64 {
                core::hint::spin_loop();
            }
        }

        // Wait for AP to signal it's alive (up to 100ms)
        let mut waited = 0u32;
        while data.ap_ready.load(Ordering::SeqCst) == 0 && waited < 100_000 {
            spin_delay_us(100);
            waited += 100;
        }

        let ready_val = data.ap_ready.load(Ordering::SeqCst);
        if ready_val != 0 {
            info!("AP {} started (APIC ID {})", apic_id, ready_val);
        } else {
            // Check if the AP wrote anything by reading raw memory
            let raw_ptr = phys_to_virt(TRAMPOLINE_PHYS + TRAMPOLINE_DATA_OFFSET + 36) as *const u32;
            let raw_val = unsafe { core::ptr::read_volatile(raw_ptr) };
            warn!(
                "AP {} did not respond (ap_ready={}, raw@0x8122={:#x})",
                apic_id, ready_val, raw_val
            );
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

/// Add an identity mapping (virt == phys) for a single 4K page in the
/// page table rooted at `pml4_phys`. Used to identity-map the trampoline
/// page so the AP can enable paging without faulting.
fn add_identity_mapping(pml4_phys: usize, phys_addr: usize) {
    use crate::mem::phys_to_virt;

    let page = phys_addr & !0xFFF;

    // Walk PML4 -> PDPT -> PD -> PT, creating entries as needed
    let pml4 = unsafe { core::slice::from_raw_parts_mut(phys_to_virt(pml4_phys) as *mut u64, 512) };

    let pml4_idx = (page >> 39) & 0x1FF;
    if pml4[pml4_idx] == 0 {
        let frame = alloc_zeroed_page();
        pml4[pml4_idx] = frame as u64 | 0x3; // present + writable
    }
    let pdpt_phys = (pml4[pml4_idx] & !0xFFF) as usize;

    let pdpt = unsafe { core::slice::from_raw_parts_mut(phys_to_virt(pdpt_phys) as *mut u64, 512) };
    let pdpt_idx = (page >> 30) & 0x1FF;
    if pdpt[pdpt_idx] == 0 {
        let frame = alloc_zeroed_page();
        pdpt[pdpt_idx] = frame as u64 | 0x3;
    }
    let pd_phys = (pdpt[pdpt_idx] & !0xFFF) as usize;

    let pd = unsafe { core::slice::from_raw_parts_mut(phys_to_virt(pd_phys) as *mut u64, 512) };
    let pd_idx = (page >> 21) & 0x1FF;
    if pd[pd_idx] == 0 {
        let frame = alloc_zeroed_page();
        pd[pd_idx] = frame as u64 | 0x3;
    }
    let pt_phys = (pd[pd_idx] & !0xFFF) as usize;

    let pt = unsafe { core::slice::from_raw_parts_mut(phys_to_virt(pt_phys) as *mut u64, 512) };
    let pt_idx = (page >> 12) & 0x1FF;
    pt[pt_idx] = page as u64 | 0x3; // present + writable, identity mapped

    info!(
        "Identity mapped phys {:#x} -> virt {:#x} (PML4[{}] PDPT[{}] PD[{}] PT[{}])",
        page, page, pml4_idx, pdpt_idx, pd_idx, pt_idx
    );
}

/// Static pool of zeroed pages for SMP page table entries.
/// We need at most 3 pages (PML4 entry -> PDPT -> PD -> PT) for the
/// identity mapping. These are in BSS, so we need their physical
/// addresses. We find the physical address via the phys_to_virt mapping:
/// the BSS virtual addresses are NOT in the phys_to_virt region, but
/// they ARE backed by physical memory that the bootloader allocated.
/// We can find the physical address by walking the existing page table.
///
/// Simpler approach: allocate pages from the physical memory region
/// that IS identity-mapped via phys_to_virt. We do this by allocating
/// from the physical frame allocator via a callback.
static mut PT_PAGE_POOL: [[u8; 4096]; 3] = [[0u8; 4096]; 3];
static mut PT_PAGE_NEXT: usize = 0;

/// Allocate a zeroed page and return its physical address.
/// Uses a static pool and looks up the physical address by walking
/// the BSP's page table.
fn alloc_zeroed_page() -> usize {
    let idx = unsafe {
        let i = PT_PAGE_NEXT;
        PT_PAGE_NEXT += 1;
        i
    };
    assert!(idx < 3, "SMP page table pool exhausted");
    let virt = unsafe { core::ptr::addr_of_mut!(PT_PAGE_POOL[idx]) as usize };
    // Look up the physical address by walking the current page table
    virt_to_phys(virt)
}

/// Translate a virtual address to physical by walking the current page table.
fn virt_to_phys(vaddr: usize) -> usize {
    let cr3: usize;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
    }
    let pml4 =
        unsafe { core::slice::from_raw_parts((phys_to_virt(cr3 & !0xFFF)) as *const u64, 512) };
    let pml4e = pml4[(vaddr >> 39) & 0x1FF];
    assert!(pml4e & 1 != 0, "PML4 entry not present for {:#x}", vaddr);

    let pdpt = unsafe {
        core::slice::from_raw_parts(phys_to_virt((pml4e & !0xFFF) as usize) as *const u64, 512)
    };
    let pdpte = pdpt[(vaddr >> 30) & 0x1FF];
    assert!(pdpte & 1 != 0, "PDPT entry not present for {:#x}", vaddr);
    // Check for 1GB huge page
    if pdpte & 0x80 != 0 {
        return ((pdpte & !0x3FFFFFFF) as usize) | (vaddr & 0x3FFFFFFF);
    }

    let pd = unsafe {
        core::slice::from_raw_parts(phys_to_virt((pdpte & !0xFFF) as usize) as *const u64, 512)
    };
    let pde = pd[(vaddr >> 21) & 0x1FF];
    assert!(pde & 1 != 0, "PD entry not present for {:#x}", vaddr);
    // Check for 2MB huge page
    if pde & 0x80 != 0 {
        return ((pde & !0x1FFFFF) as usize) | (vaddr & 0x1FFFFF);
    }

    let pt = unsafe {
        core::slice::from_raw_parts(phys_to_virt((pde & !0xFFF) as usize) as *const u64, 512)
    };
    let pte = pt[(vaddr >> 12) & 0x1FF];
    assert!(pte & 1 != 0, "PT entry not present for {:#x}", vaddr);
    ((pte & !0xFFF) as usize) | (vaddr & 0xFFF)
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

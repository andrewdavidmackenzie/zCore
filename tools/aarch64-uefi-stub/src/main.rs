//! UEFI boot stub for zCore aarch64.
//!
//! Loads the kernel ELF from the ESP into its linked physical address
//! (using UEFI AllocatePages at a specific address), collects boot info,
//! exits UEFI, installs page tables, and jumps to the kernel.
//!
//! The kernel is linked at virtual 0xffff_0000_4008_0000, which maps
//! to physical 0x4008_0000. We ask UEFI to allocate pages at that
//! physical address, then load the ELF segments directly there.
//! No relocation or copying needed.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::arch::asm;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;

// ── Constants ────────────────────────────────────────────────────────

const UART_BASE: *mut u8 = 0x0900_0000 as *mut u8;
/// Physical-to-virtual offset. The kernel virtual address space starts
/// at 0xffff_0000_0000_0000. Must match the linker script.
const PHYS_TO_VIRT_OFFSET: u64 = 0xffff_0000_0000_0000;
const KERNEL_PATH: &str = "\\kernel";
const INITRD_PATH: &str = "\\initrd.img";
/// DTB path on the ESP (loaded if UEFI config tables don't have one).
const DTB_PATH: &str = "\\virt.dtb";

// ── UART helpers ─────────────────────────────────────────────────────

fn uart_putc(c: u8) {
    unsafe { core::ptr::write_volatile(UART_BASE, c) };
}
fn uart_puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}
fn uart_put_hex(val: u64) {
    let hex = b"0123456789abcdef";
    for i in (0..16).rev() {
        uart_putc(hex[((val >> (i * 4)) & 0xf) as usize]);
    }
}
fn uart_put_dec(mut val: u64) {
    if val == 0 {
        uart_putc(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while val > 0 {
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        uart_putc(buf[i]);
    }
}

// ── ELF types ────────────────────────────────────────────────────────

#[repr(C)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const PT_LOAD: u32 = 1;

#[repr(C)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

#[repr(C)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;

// ── Page table types ─────────────────────────────────────────────────

#[repr(C, align(4096))]
struct PageTable([u64; 512]);
impl PageTable {
    const fn zeroed() -> Self {
        Self([0u64; 512])
    }
}

const PT_VALID: u64 = 1 << 0;
const PT_TABLE: u64 = 1 << 1;
const PT_AF: u64 = 1 << 10;
const PT_SH_INNER: u64 = 3 << 8;
const PT_ATTR_NORMAL: u64 = 1 << 2;
const PT_ATTR_DEVICE: u64 = 0 << 2;

fn block_desc(phys: u64, device: bool) -> u64 {
    let attr = if device {
        PT_ATTR_DEVICE
    } else {
        PT_ATTR_NORMAL | PT_SH_INNER
    };
    phys | PT_VALID | PT_AF | attr
}
fn table_desc(table_phys: u64) -> u64 {
    table_phys | PT_VALID | PT_TABLE
}

// Page tables in BSS — these are in the UEFI stub's address space,
// which is above the kernel load region. They survive the kernel load.
static mut PT_L0_LO: PageTable = PageTable::zeroed();
static mut PT_L0_HI: PageTable = PageTable::zeroed();
static mut PT_L1_ID: PageTable = PageTable::zeroed();
static mut PT_L1_HI: PageTable = PageTable::zeroed();

// ── Main ─────────────────────────────────────────────────────────────

#[entry]
fn main() -> Status {
    uart_puts("zCore UEFI stub starting on aarch64\n");

    // 1. Load kernel ELF from ESP into a temporary buffer
    let kernel_data = match load_file(KERNEL_PATH) {
        Some(d) => {
            uart_puts("Loaded kernel ELF: ");
            uart_put_dec(d.len() as u64);
            uart_puts(" bytes\n");
            d
        }
        None => {
            uart_puts("FATAL: kernel not found at ");
            uart_puts(KERNEL_PATH);
            uart_puts("\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };

    // 2. Parse ELF and load segments at their linked physical addresses.
    //    Uses UEFI AllocatePages to reserve the exact physical regions
    //    the kernel expects, then copies segments directly there.
    let entry = load_elf_at_linked_address(&kernel_data);
    uart_puts("Kernel entry: 0x");
    uart_put_hex(entry);
    uart_puts("\n");

    // Free the ELF buffer — segments are now at their final addresses.
    drop(kernel_data);

    // 3. Load initrd (optional) — allocate at a known safe address
    let (initrd_start, initrd_size) = load_initrd();

    // 4. Collect DTB — try UEFI config tables first, then ESP file
    let dtb_paddr = if let Some(addr) = find_dtb() {
        uart_puts("DTB from UEFI config table at 0x");
        uart_put_hex(addr as u64);
        uart_puts("\n");
        addr as u64
    } else if let Some(dtb_data) = load_file(DTB_PATH) {
        // Load DTB from ESP to a safe physical address
        let dtb_addr: u64 = 0x4700_0000; // below initrd
        let num_pages = (dtb_data.len() + 4095) / 4096;
        let _ = uefi::boot::allocate_pages(
            uefi::boot::AllocateType::Address(dtb_addr),
            uefi::boot::MemoryType::LOADER_DATA,
            num_pages,
        );
        unsafe {
            core::ptr::copy_nonoverlapping(
                dtb_data.as_ptr(),
                dtb_addr as *mut u8,
                dtb_data.len(),
            );
        }
        uart_puts("DTB loaded from ESP: ");
        uart_put_dec(dtb_data.len() as u64);
        uart_puts(" bytes at 0x");
        uart_put_hex(dtb_addr);
        uart_puts("\n");
        dtb_addr
    } else {
        uart_puts("WARNING: no DTB found (UEFI config tables or ESP)\n");
        0u64
    };

    // 5. Build page tables
    build_page_tables();
    uart_puts("Page tables ready\n");

    // 6. Exit boot services
    uart_puts("Exiting boot services...\n");
    let _ = unsafe { uefi::boot::exit_boot_services(Some(uefi::boot::MemoryType::LOADER_DATA)) };
    uart_puts("Boot services exited\n");

    // 7. Install page tables and jump to kernel
    unsafe {
        install_page_tables_and_jump(entry, dtb_paddr, initrd_start, initrd_size);
    }
}

// ── ELF loading at linked address ────────────────────────────────────

/// Parse ELF and load PT_LOAD segments at their linked physical addresses.
/// Uses UEFI AllocatePages to reserve the target memory regions.
fn load_elf_at_linked_address(data: &[u8]) -> u64 {
    let hdr = unsafe { &*(data.as_ptr() as *const Elf64Header) };
    if &hdr.e_ident[0..4] != b"\x7fELF" || hdr.e_machine != 0xB7 {
        uart_puts("FATAL: invalid aarch64 ELF\n");
        loop {
            core::hint::spin_loop();
        }
    }

    let ph_offset = hdr.e_phoff as usize;
    let ph_size = hdr.e_phentsize as usize;

    for i in 0..hdr.e_phnum as usize {
        let phdr = unsafe { &*(data.as_ptr().add(ph_offset + i * ph_size) as *const Elf64Phdr) };
        if phdr.p_type != PT_LOAD || phdr.p_memsz == 0 {
            continue;
        }

        let paddr = if phdr.p_vaddr >= PHYS_TO_VIRT_OFFSET {
            phdr.p_vaddr - PHYS_TO_VIRT_OFFSET
        } else {
            phdr.p_vaddr
        };

        uart_puts("  LOAD: paddr=0x");
        uart_put_hex(paddr);
        uart_puts(" filesz=0x");
        uart_put_hex(phdr.p_filesz);
        uart_puts(" memsz=0x");
        uart_put_hex(phdr.p_memsz);
        uart_puts("\n");

        // Allocate pages at the exact physical address
        let num_pages = (phdr.p_memsz as usize + 4095) / 4096;
        let page_addr = paddr & !0xFFF; // page-align down
        match uefi::boot::allocate_pages(
            uefi::boot::AllocateType::Address(page_addr as u64),
            uefi::boot::MemoryType::LOADER_DATA,
            num_pages,
        ) {
            Ok(_) => {}
            Err(e) => {
                uart_puts("  WARNING: allocate_pages at 0x");
                uart_put_hex(page_addr);
                uart_puts(" failed: ");
                uart_put_hex(e.status().0 as u64);
                uart_puts(" (continuing anyway)\n");
                // Continue — the memory might already be available as
                // conventional memory without explicit allocation.
            }
        }

        // Copy file data to physical address
        if phdr.p_filesz > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(phdr.p_offset as usize),
                    paddr as *mut u8,
                    phdr.p_filesz as usize,
                );
            }
        }
        // Zero BSS
        if phdr.p_memsz > phdr.p_filesz {
            unsafe {
                core::ptr::write_bytes(
                    (paddr + phdr.p_filesz) as *mut u8,
                    0,
                    (phdr.p_memsz - phdr.p_filesz) as usize,
                );
            }
        }
    }

    // Find rust_main_uefi symbol for UEFI entry
    let uefi_entry = find_symbol(data, hdr, "rust_main_uefi");
    match uefi_entry {
        Some(addr) => {
            uart_puts("Found rust_main_uefi at 0x");
            uart_put_hex(addr);
            uart_puts("\n");
            addr
        }
        None => {
            uart_puts("WARNING: rust_main_uefi not found, using ELF entry\n");
            hdr.e_entry
        }
    }
}

/// Find a symbol by name in the ELF symbol table.
fn find_symbol(data: &[u8], hdr: &Elf64Header, name: &str) -> Option<u64> {
    let sh_offset = hdr.e_shoff as usize;
    let sh_size = hdr.e_shentsize as usize;
    let sh_num = hdr.e_shnum as usize;

    // Find .symtab section
    for i in 0..sh_num {
        let shdr = unsafe { &*(data.as_ptr().add(sh_offset + i * sh_size) as *const Elf64Shdr) };
        if shdr.sh_type != SHT_SYMTAB {
            continue;
        }

        // Get the linked string table
        let strtab_shdr = unsafe {
            &*(data
                .as_ptr()
                .add(sh_offset + shdr.sh_link as usize * sh_size)
                as *const Elf64Shdr)
        };
        let strtab = &data[strtab_shdr.sh_offset as usize..];

        // Iterate symbols
        let sym_count = shdr.sh_size as usize / shdr.sh_entsize as usize;
        for j in 0..sym_count {
            let sym = unsafe {
                &*(data
                    .as_ptr()
                    .add(shdr.sh_offset as usize + j * shdr.sh_entsize as usize)
                    as *const Elf64Sym)
            };
            let sym_name_offset = sym.st_name as usize;
            if sym_name_offset >= strtab.len() {
                continue;
            }
            // Compare null-terminated string
            let sym_name = &strtab[sym_name_offset..];
            let name_bytes = name.as_bytes();
            if sym_name.len() > name_bytes.len()
                && sym_name[..name_bytes.len()] == *name_bytes
                && sym_name[name_bytes.len()] == 0
            {
                return Some(sym.st_value);
            }
        }
    }
    None
}

// ── Initrd loading ───────────────────────────────────────────────────

/// Load initrd from ESP to a safe physical address (above kernel).
fn load_initrd() -> (u64, u64) {
    let data = match load_file(INITRD_PATH) {
        Some(d) => d,
        None => {
            uart_puts("No initrd found (optional)\n");
            return (0, 0);
        }
    };
    uart_puts("Initrd: ");
    uart_put_dec(data.len() as u64);
    uart_puts(" bytes\n");

    // Allocate pages for the initrd above the kernel region.
    // Use 0x4800_0000 (above the kernel's ~6 MB footprint starting at 0x4008_0000).
    let initrd_paddr: u64 = 0x4800_0000;
    let num_pages = (data.len() + 4095) / 4096;
    let _ = uefi::boot::allocate_pages(
        uefi::boot::AllocateType::Address(initrd_paddr),
        uefi::boot::MemoryType::LOADER_DATA,
        num_pages,
    );
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), initrd_paddr as *mut u8, data.len());
    }
    uart_puts("Initrd at 0x");
    uart_put_hex(initrd_paddr);
    uart_puts("\n");
    (initrd_paddr, data.len() as u64)
}

// ── Page tables ──────────────────────────────────────────────────────

fn build_page_tables() {
    unsafe {
        // L1 identity: 0-1G device, 1-2G normal, 2-3G normal
        PT_L1_ID.0[0] = block_desc(0x0000_0000, true);
        PT_L1_ID.0[1] = block_desc(0x4000_0000, false);
        PT_L1_ID.0[2] = block_desc(0x8000_0000, false);
        // L1 high: same
        PT_L1_HI.0[0] = block_desc(0x0000_0000, true);
        PT_L1_HI.0[1] = block_desc(0x4000_0000, false);
        PT_L1_HI.0[2] = block_desc(0x8000_0000, false);
        // L0 tables
        PT_L0_LO.0[0] = table_desc(core::ptr::addr_of!(PT_L1_ID) as u64);
        PT_L0_HI.0[0] = table_desc(core::ptr::addr_of!(PT_L1_HI) as u64);
    }
}

// ── Jump to kernel ───────────────────────────────────────────────────

unsafe fn install_page_tables_and_jump(
    entry: u64,
    dtb_paddr: u64,
    _initrd_start: u64,
    _initrd_size: u64,
) -> ! {
    // Disable MMU (UEFI left it on with its own page tables)
    asm!(
        "mrs x1, sctlr_el1",
        "bic x1, x1, #1",
        "msr sctlr_el1, x1",
        "isb",
        out("x1") _,
    );

    // Set MAIR: Attr0=0x04 (Device-nGnRE), Attr1=0xFF (Normal WB)
    // Must match boot.s
    asm!("mov x1, #0xFF04", "msr mair_el1, x1", "isb", out("x1") _);

    // Set TCR — must match boot.s
    asm!(
        "ldr x1, ={tcr}",
        "msr tcr_el1, x1",
        "isb",
        tcr = const 0x0000_0002_B510_3510u64,
        out("x1") _,
    );

    // Install page tables
    let ttbr0 = core::ptr::addr_of!(PT_L0_LO) as u64;
    let ttbr1 = core::ptr::addr_of!(PT_L0_HI) as u64;
    asm!(
        "msr ttbr0_el1, {t0}",
        "msr ttbr1_el1, {t1}",
        "isb",
        "tlbi vmalle1is",
        "dsb sy",
        "isb",
        t0 = in(reg) ttbr0,
        t1 = in(reg) ttbr1,
    );

    // Enable MMU + caches
    asm!(
        "mrs x1, sctlr_el1",
        "orr x1, x1, #1",
        "orr x1, x1, #(1<<2)",
        "orr x1, x1, #(1<<12)",
        "msr sctlr_el1, x1",
        "isb",
        out("x1") _,
    );

    uart_puts("Jumping to kernel\n");

    // Jump to kernel. rust_main_uefi(dtb_paddr) in x0.
    // Use explicit registers to avoid compiler register conflicts.
    asm!(
        "br x1",
        in("x0") dtb_paddr,
        in("x1") entry,
        options(noreturn),
    );
}

// ── File loading ─────────────────────────────────────────────────────

fn load_file(path: &str) -> Option<Vec<u8>> {
    let handle = uefi::boot::get_handle_for_protocol::<SimpleFileSystem>().ok()?;
    let mut fs = uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(handle).ok()?;
    let mut root = fs.open_volume().ok()?;
    let mut ucs2_buf = [0u16; 64];
    let ucs2_len = path
        .chars()
        .enumerate()
        .map(|(i, c)| {
            ucs2_buf[i] = c as u16;
            i
        })
        .last()
        .map(|i| i + 1)
        .unwrap_or(0);
    let cstr = uefi::CStr16::from_u16_with_nul(&ucs2_buf[..=ucs2_len]).ok()?;
    let file_handle = root
        .open(cstr, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file: RegularFile = file_handle.into_regular_file()?;
    let mut info_buf = [0u8; 256];
    let info = file.get_info::<FileInfo>(&mut info_buf).ok()?;
    let size = info.file_size() as usize;
    let mut data = alloc::vec![0u8; size];
    file.read(&mut data).ok()?;
    Some(data)
}

// ── DTB / GOP ────────────────────────────────────────────────────────

fn dump_config_tables() {
    let st = match uefi::table::system_table_raw() {
        Some(st) => unsafe { st.as_ref() },
        None => return,
    };
    let entries = unsafe {
        core::slice::from_raw_parts(
            st.configuration_table,
            st.number_of_configuration_table_entries,
        )
    };
    uart_puts("Config tables (");
    uart_put_dec(entries.len() as u64);
    uart_puts("):\n");
    for entry in entries {
        let bytes = entry.vendor_guid.to_bytes();
        uart_puts("  ");
        for b in bytes {
            let hex = b"0123456789abcdef";
            uart_putc(hex[(b >> 4) as usize]);
            uart_putc(hex[(b & 0xf) as usize]);
        }
        uart_puts(" -> 0x");
        uart_put_hex(entry.vendor_table as u64);
        uart_puts("\n");
    }
}

fn find_dtb() -> Option<usize> {
    let dtb_guid = uefi::guid!("b1b621d5-f19c-41a5-830b-d9152c69aae0");
    let st = uefi::table::system_table_raw()?;
    let st = unsafe { st.as_ref() };
    let entries = unsafe {
        core::slice::from_raw_parts(
            st.configuration_table,
            st.number_of_configuration_table_entries,
        )
    };
    for entry in entries {
        if entry.vendor_guid == dtb_guid {
            return Some(entry.vendor_table as usize);
        }
    }
    None
}

#[allow(dead_code)]
fn get_gop_info() -> Option<(u64, u32, u32, u32)> {
    let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;
    let mode = gop.current_mode_info();
    let (w, h) = mode.resolution();
    let base = gop.frame_buffer().as_mut_ptr() as u64;
    Some((base, w as u32, h as u32, mode.stride() as u32))
}

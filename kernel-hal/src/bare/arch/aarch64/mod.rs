pub mod config;
pub mod cpu;
pub mod drivers;
pub mod interrupt;
pub mod mem;
pub mod timer;
pub mod trap;
pub mod vm;

use crate::KCONFIG;
use crate::{mem::phys_to_virt, utils::init_once::InitOnce, PhysAddr};
use alloc::string::{String, ToString};
use core::ops::Range;

hal_fn_impl! {
    impl mod crate::hal_fn::console {
        fn console_write_early(s: &str) {
            // Write directly to PL011 UART data register at virtual address.
            #[cfg(feature = "board-raspi400")]
            const UART_VIRT: usize = 0xFFFF_0000_FE20_1000;
            #[cfg(not(feature = "board-raspi400"))]
            const UART_VIRT: usize = 0xFFFF_0000_0900_0000;

            let uart = UART_VIRT as *mut u32;
            let fr = (UART_VIRT + 0x18) as *const u32;
            for c in s.bytes() {
                unsafe {
                    #[cfg(feature = "board-raspi400")]
                    if c == b'\n' {
                        while core::ptr::read_volatile(fr) & (1 << 5) != 0 {}
                        core::ptr::write_volatile(uart, b'\r' as u32);
                    }
                    // Wait for TX FIFO not full (UARTFR bit 5 = TXFF)
                    while core::ptr::read_volatile(fr) & (1 << 5) != 0 {}
                    core::ptr::write_volatile(uart, c as u32);
                }
            }
        }
    }
}

static INITRD_REGION: InitOnce<Option<Range<PhysAddr>>> = InitOnce::new_with_default(None);
static CMDLINE: InitOnce<String> = InitOnce::new_with_default(String::new());
/// DTB-discovered memory end address. If set, overrides the compile-time
/// PHYS_MEMORY_END constant in free_pmem_regions().
static DTB_MEMORY_END: InitOnce<Option<usize>> = InitOnce::new_with_default(None);
/// DTB-discovered UART base address.
static DTB_UART_BASE: InitOnce<Option<usize>> = InitOnce::new_with_default(None);
/// DTB-discovered GIC base address (distributor).
static DTB_GIC_BASE: InitOnce<Option<usize>> = InitOnce::new_with_default(None);

/// Get the physical memory end address, preferring DTB-discovered value.
pub fn phys_memory_end() -> usize {
    match *DTB_MEMORY_END {
        Some(end) => end,
        None => config::PHYS_MEMORY_END,
    }
}

pub fn cmdline() -> String {
    CMDLINE.clone()
}

pub fn init_ram_disk() -> Option<&'static mut [u8]> {
    INITRD_REGION.as_ref().map(|range| unsafe {
        core::slice::from_raw_parts_mut(phys_to_virt(range.start) as *mut u8, range.len())
    })
}

pub fn primary_init_early() {
    // Parse DTB for bootargs and initrd if a valid DTB was provided.
    let dtb_paddr = KCONFIG.dtb_paddr;
    if dtb_paddr != 0 {
        parse_dtb(dtb_paddr);
    } else {
        CMDLINE.init_once_by(KCONFIG.cmdline.to_string());
    }
    drivers::init_early();
}

/// Get the UART base address, preferring DTB-discovered value.
pub fn uart_base() -> usize {
    match *DTB_UART_BASE {
        Some(base) => base,
        None => KCONFIG.uart_base,
    }
}

/// Get the GIC base address, preferring DTB-discovered value.
pub fn gic_base() -> usize {
    match *DTB_GIC_BASE {
        Some(base) => base,
        None => KCONFIG.gic_base,
    }
}

/// Information discovered from the DTB.
struct DtbInfo {
    bootargs: Option<String>,
    initrd_start: Option<usize>,
    initrd_end: Option<usize>,
    memory_base: Option<usize>,
    memory_size: Option<usize>,
    uart_base: Option<usize>,
    gic_base: Option<usize>,
}

/// Parse the DTB to extract bootargs, initrd, and hardware info.
fn parse_dtb(dtb_paddr: usize) {
    use dtb_walker::{Dtb, DtbObj, Property, Str, WalkOperation::*};

    let dtb_vaddr = phys_to_virt(dtb_paddr);
    let dtb = unsafe {
        Dtb::from_raw_parts_filtered(dtb_vaddr as _, |e| {
            log::warn!("DTB parse error: {:?}", e);
            false
        })
    };
    let dtb = match dtb {
        Ok(dtb) => dtb,
        Err(e) => {
            log::warn!("DTB parse failed: {:?}", e);
            log::warn!("Failed to parse DTB at {:#x}", dtb_paddr);
            CMDLINE.init_once_by(KCONFIG.cmdline.to_string());
            return;
        }
    };

    log::info!("DTB at {:#x}, size={}", dtb_paddr, dtb.total_size());

    let mut info = DtbInfo {
        bootargs: None,
        initrd_start: None,
        initrd_end: None,
        memory_base: None,
        memory_size: None,
        uart_base: None,
        gic_base: None,
    };

    // Track which top-level node we're inside for property context.
    // The dtb_walker doesn't provide node-property association, so
    // we track it manually via the node name.
    let mut current_node: [u8; 64] = [0; 64];
    let mut current_node_len: usize = 0;
    let mut node_depth: usize = 0;

    dtb.walk(|path, obj| match obj {
        DtbObj::SubNode { name } => {
            let name_bytes = name.as_bytes();
            if path.is_root() {
                // Save top-level node name for property context
                current_node_len = name_bytes.len().min(64);
                current_node[..current_node_len].copy_from_slice(&name_bytes[..current_node_len]);
                node_depth = 1;
                // Step into nodes we care about
                if name_bytes.starts_with(b"chosen")
                    || name_bytes.starts_with(b"memory")
                    || name_bytes.starts_with(b"pl011")
                    || name_bytes.starts_with(b"uart")
                    || name_bytes.starts_with(b"serial")
                    || name_bytes.starts_with(b"intc")
                    || name_bytes.starts_with(b"interrupt-controller")
                {
                    return StepInto;
                }
                // Also step into soc/ to find nested devices
                if name_bytes == b"soc" {
                    return StepInto;
                }
            } else if node_depth == 1 {
                // Inside /soc -- look for UART and interrupt controller
                current_node_len = name_bytes.len().min(64);
                current_node[..current_node_len].copy_from_slice(&name_bytes[..current_node_len]);
                node_depth = 2;
                if name_bytes.starts_with(b"pl011")
                    || name_bytes.starts_with(b"uart")
                    || name_bytes.starts_with(b"serial")
                    || name_bytes.starts_with(b"intc")
                    || name_bytes.starts_with(b"interrupt-controller")
                {
                    return StepInto;
                }
            }
            StepOver
        }
        DtbObj::Property(Property::General { name, value }) => {
            let ctx = &current_node[..current_node_len];

            if ctx.starts_with(b"chosen") {
                if name == Str::from("bootargs") {
                    if let Ok(s) = core::str::from_utf8(value) {
                        let s = s.trim_end_matches('\0');
                        log::info!("DTB bootargs: {:?}", s);
                        info.bootargs = Some(s.to_string());
                    }
                } else if name == Str::from("linux,initrd-start") {
                    info.initrd_start = parse_dtb_u64(value);
                    log::info!("DTB initrd-start: {:#x?}", info.initrd_start);
                } else if name == Str::from("linux,initrd-end") {
                    info.initrd_end = parse_dtb_u64(value);
                    log::info!("DTB initrd-end: {:#x?}", info.initrd_end);
                }
            } else if ctx.starts_with(b"memory") && name == Str::from("reg") {
                if value.len() >= 16 {
                    let base = u64::from_be_bytes(value[0..8].try_into().unwrap()) as usize;
                    let size = u64::from_be_bytes(value[8..16].try_into().unwrap()) as usize;
                    log::info!(
                        "DTB memory: base={:#x}, size={:#x} ({} MiB)",
                        base,
                        size,
                        size >> 20
                    );
                    info.memory_base = Some(base);
                    info.memory_size = Some(size);
                } else if value.len() >= 8 {
                    let base = u32::from_be_bytes(value[0..4].try_into().unwrap()) as usize;
                    let size = u32::from_be_bytes(value[4..8].try_into().unwrap()) as usize;
                    log::info!(
                        "DTB memory: base={:#x}, size={:#x} ({} MiB)",
                        base,
                        size,
                        size >> 20
                    );
                    info.memory_base = Some(base);
                    info.memory_size = Some(size);
                }
            } else if name == Str::from("compatible") {
                // Check for PL011 UART
                if value.windows(9).any(|w| w == b"arm,pl011") && info.uart_base.is_none() {
                    // Extract address from node name: "pl011@ADDR" or "serial@ADDR"
                    if let Some(addr) = parse_node_addr(ctx) {
                        log::info!("DTB UART (PL011): {:#x}", addr);
                        info.uart_base = Some(addr);
                    }
                }
                // Check for GIC-400 or compatible GIC
                if (value.windows(11).any(|w| w == b"arm,gic-400")
                    || value.windows(19).any(|w| w == b"arm,cortex-a15-gic"))
                    && info.gic_base.is_none()
                {
                    if let Some(addr) = parse_node_addr(ctx) {
                        log::info!("DTB GIC: {:#x}", addr);
                        info.gic_base = Some(addr);
                    }
                }
            }
            StepOver
        }
        _ => StepOver,
    });

    // Use DTB bootargs if available, otherwise fall back to compile-time
    let cmdline = info.bootargs.unwrap_or_else(|| KCONFIG.cmdline.to_string());
    CMDLINE.init_once_by(cmdline);

    // Set initrd region if both start and end are provided
    if let (Some(start), Some(end)) = (info.initrd_start, info.initrd_end) {
        if end > start {
            log::info!(
                "DTB initrd: {:#x}..{:#x} ({} bytes)",
                start,
                end,
                end - start
            );
            INITRD_REGION.init_once_by(Some(start..end));
        }
    }

    // Store DTB-discovered UART and GIC addresses.
    if let Some(uart) = info.uart_base {
        DTB_UART_BASE.init_once_by(Some(uart));
    }
    if let Some(gic) = info.gic_base {
        DTB_GIC_BASE.init_once_by(Some(gic));
    }

    // Use DTB-discovered memory to override compile-time defaults.
    // Cap at a reasonable limit to avoid mapping issues with the boot
    // page tables (which only cover 3-4 GiB depending on board).
    if let (Some(base), Some(size)) = (info.memory_base, info.memory_size) {
        let end = base + size;
        // Cap usable memory at 1 GiB from kernel end to stay within
        // the boot page table mappings. The 4K remap in vm::init()
        // will eventually map all physical memory properly.
        let kernel_end = {
            extern "C" {
                fn ekernel();
            }
            ekernel as *const () as usize & config::PHYS_ADDR_MASK
        };
        let capped_end = end.min(kernel_end + 1024 * 1024 * 1024);
        log::info!(
            "DTB memory: {:#x}..{:#x}, usable end capped to {:#x}",
            base,
            end,
            capped_end
        );
        DTB_MEMORY_END.init_once_by(Some(capped_end));
    }
}

/// Parse a DTB property value as a u64 (big-endian, 4 or 8 bytes).
fn parse_dtb_u64(value: &[u8]) -> Option<usize> {
    match value.len() {
        4 => Some(u32::from_be_bytes(value.try_into().ok()?) as usize),
        8 => Some(u64::from_be_bytes(value.try_into().ok()?) as usize),
        _ => None,
    }
}

/// Parse an address from a DTB node name like "serial@9000000" or "intc@8000000".
fn parse_node_addr(name: &[u8]) -> Option<usize> {
    let at_pos = name.iter().position(|&b| b == b'@')?;
    let addr_str = core::str::from_utf8(&name[at_pos + 1..]).ok()?;
    usize::from_str_radix(addr_str, 16).ok()
}

pub fn primary_init() {
    vm::init();
    drivers::init();
}

pub fn secondary_init() {
    unimplemented!()
}

pub const fn timer_interrupt_vector() -> usize {
    #[cfg(feature = "board-raspi400")]
    { 27 }
    #[cfg(not(feature = "board-raspi400"))]
    { 30 }
}

pub fn timer_init() {
    timer::init();
}

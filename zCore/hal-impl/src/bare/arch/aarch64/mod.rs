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

/// Board-default UART base address, set by the platform entry point.
/// Overridden by DTB discovery if available.
static BOARD_UART_BASE: InitOnce<usize> = InitOnce::new();
/// Board-default GIC base address, set by the platform entry point.
/// Overridden by DTB discovery if available.
static BOARD_GIC_BASE: InitOnce<usize> = InitOnce::new();

/// Set the board-default UART and GIC base addresses.
/// Called from the platform entry point before `primary_init_early()`.
pub fn set_board_bases(uart_base: usize, gic_base: usize) {
    BOARD_UART_BASE.init_once_by(uart_base);
    BOARD_GIC_BASE.init_once_by(gic_base);
}

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
static DTB_CPU_COUNT: InitOnce<usize> = InitOnce::new_with_default(1);

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

    // If DTB didn't provide initrd location, use KernelConfig
    // (set by UEFI stub via UefiBootInfo).
    if INITRD_REGION.as_ref().is_none() && KCONFIG.initrd_start != 0 && KCONFIG.initrd_size != 0 {
        let start = KCONFIG.initrd_start as usize;
        let end = start + KCONFIG.initrd_size as usize;
        log::info!("Initrd from boot info: {:#x}..{:#x}", start, end);
        INITRD_REGION.init_once_by(Some(start..end));
    }

    drivers::init_early();
}

/// Get the UART base address, preferring DTB-discovered value.
pub fn uart_base() -> usize {
    match *DTB_UART_BASE {
        Some(base) => base,
        None => *BOARD_UART_BASE,
    }
}

/// Get the GIC base address, preferring DTB-discovered value.
pub fn gic_base() -> usize {
    match *DTB_GIC_BASE {
        Some(base) => base,
        None => *BOARD_GIC_BASE,
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
    cpu_count: usize,
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
        cpu_count: 0,
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
                // Also step into soc/ and cpus/ to find nested devices
                if name_bytes == b"soc" || name_bytes == b"cpus" {
                    current_node_len = name_bytes.len().min(64);
                    current_node[..current_node_len]
                        .copy_from_slice(&name_bytes[..current_node_len]);
                    node_depth = 1;
                    return StepInto;
                }
            } else if node_depth >= 1 {
                // Inside /cpus (depth 1 or 2) -- count cpu@N nodes
                if name_bytes.starts_with(b"cpu@") {
                    info.cpu_count += 1;
                    return StepOver;
                }
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
        DtbObj::Property(Property::Reg(reg)) => {
            let ctx = &current_node[..current_node_len];
            if ctx.starts_with(b"memory") {
                for range in reg {
                    let base = range.start;
                    let size = range.end - range.start;
                    log::info!(
                        "DTB memory (reg): base={:#x}, size={:#x} ({} MiB)",
                        base,
                        size,
                        size >> 20
                    );
                    if info.memory_base.is_none() {
                        info.memory_base = Some(base);
                        info.memory_size = Some(size);
                    }
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

    // Merge DTB bootargs with compile-time cmdline.
    // DTB bootargs take precedence for keys that appear in both;
    // compile-time keys (like LOG=) are appended if not in DTB.
    let cmdline = match info.bootargs {
        Some(dtb_args) => {
            let mut merged = dtb_args.clone();
            // Append compile-time cmdline keys not already in DTB bootargs
            for token in KCONFIG.cmdline.split_whitespace() {
                if let Some(key) = token.split('=').next() {
                    if !dtb_args.split_whitespace().any(|t| t.starts_with(key)) {
                        merged.push(' ');
                        merged.push_str(token);
                    }
                }
            }
            merged
        }
        None => KCONFIG.cmdline.to_string(),
    };
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
    if info.cpu_count > 0 {
        DTB_CPU_COUNT.init_once_by(info.cpu_count);
        log::info!("DTB: {} CPU(s) detected", info.cpu_count);
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
    // Start secondary cores (QEMU virt uses PSCI).
    // UEFI boot: SMP not yet implemented (single-core for now).
    #[cfg(all(not(feature = "board-raspi400"), not(feature = "uefi-boot")))]
    start_secondary_cores();
}

/// Start secondary cores via PSCI CPU_ON.
///
/// On QEMU virt, secondary cores are powered off at boot. We use
/// PSCI CPU_ON to start each one at the `_secondary_entry` physical
/// address. The secondary entry assembly enables the MMU and jumps
/// to `secondary_core_init` in Rust.
#[cfg(all(not(feature = "board-raspi400"), not(feature = "uefi-boot")))]
fn start_secondary_cores() {
    extern "C" {
        fn _secondary_entry();
    }
    // The entry point must be a physical address.
    // _secondary_entry is linked at a virtual address; subtract the offset.
    let phys_to_virt_offset = crate::KCONFIG.phys_to_virt_offset;
    let entry_vaddr = _secondary_entry as *const () as usize;
    let entry_paddr = entry_vaddr - phys_to_virt_offset;

    // Start secondary cores (core 0 is the BSP).
    // CPU count comes from DTB; only start cores that actually exist.
    let cpu_count = *DTB_CPU_COUNT;
    for core_id in 1..cpu_count as u64 {
        info!(
            "Starting secondary core {} at paddr {:#x}",
            core_id, entry_paddr
        );
        match cpu::psci_cpu_on(core_id as usize, entry_paddr, 0) {
            Ok(()) => info!("Core {} started successfully", core_id),
            Err(e) => warn!("Failed to start core {}: PSCI error {}", core_id, e),
        }
    }
}

/// Per-core initialization for secondary (AP) cores.
///
/// Called after the secondary core has set up its stack and enabled
/// the MMU. Initializes per-core hardware: GIC CPU interface and
/// generic timer.
pub fn secondary_init() {
    // Initialize the per-core GIC CPU interface (banked registers)
    drivers::init_secondary_gic();
    // Enable the per-core timer
    timer::init();
    info!("secondary core {} initialized", cpu::cpu_id());
}

pub const fn timer_interrupt_vector() -> usize {
    #[cfg(feature = "board-raspi400")]
    {
        27
    }
    #[cfg(not(feature = "board-raspi400"))]
    {
        30
    }
}

pub fn timer_init() {
    timer::init();
}
pub mod platform;

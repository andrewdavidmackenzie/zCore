pub mod config;
pub mod cpu;
pub mod drivers;
#[cfg(feature = "board-jollac2")]
pub(crate) mod fb_console;
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
            #[cfg(feature = "board-jollac2")]
            {
                fb_console::write_str(s);
            }

            #[cfg(not(feature = "board-jollac2"))]
            {
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

/// Parse the DTB to extract bootargs, initrd, and hardware info.
///
/// Uses the shared [`Devicetree`] wrapper from the `drivers` crate
/// for all DTB parsing.
fn parse_dtb(dtb_paddr: usize) {
    use ::drivers::utils::devicetree::Devicetree;

    let dtb_vaddr = phys_to_virt(dtb_paddr);
    let dt = match Devicetree::from(dtb_vaddr) {
        Ok(dt) => dt,
        Err(e) => {
            log::error!("Failed to parse DTB at {:#x}: {:?}", dtb_paddr, e);
            CMDLINE.init_once_by(KCONFIG.cmdline.to_string());
            return;
        }
    };

    // Merge DTB bootargs with compile-time cmdline.
    // DTB bootargs take precedence for keys that appear in both;
    // compile-time keys (like LOG=) are appended if not in DTB.
    let cmdline = match dt.bootargs() {
        Some(dtb_args) => {
            let mut merged = dtb_args.to_string();
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

    // Set initrd region
    if let Some(region) = dt.initrd_region() {
        if region.end > region.start {
            log::info!(
                "DTB initrd: {:#x}..{:#x} ({} bytes)",
                region.start,
                region.end,
                region.end - region.start
            );
            INITRD_REGION.init_once_by(Some(region));
        }
    }

    // Store DTB-discovered UART and GIC addresses.
    if let Some(uart) = dt.device_address("arm,pl011") {
        log::info!("DTB UART (PL011): {:#x}", uart);
        DTB_UART_BASE.init_once_by(Some(uart));
    }
    if let Some(gic) = dt
        .device_address("arm,gic-400")
        .or_else(|| dt.device_address("arm,cortex-a15-gic"))
    {
        log::info!("DTB GIC: {:#x}", gic);
        DTB_GIC_BASE.init_once_by(Some(gic));
    }
    let cpu_count = dt.cpu_count();
    if cpu_count > 0 {
        DTB_CPU_COUNT.init_once_by(cpu_count);
        log::info!("DTB: {} CPU(s) detected", cpu_count);
    }

    // Use DTB-discovered memory to override compile-time defaults.
    if let Ok(regions) = dt.memory_regions() {
        if let Some(region) = regions.first() {
            let base = region.start;
            let end = region.end;
            // Cap usable memory at 1 GiB from kernel end to stay within
            // the boot page table mappings.
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
}

pub fn primary_init() {
    info!("primary_init: calling vm::init()...");
    vm::init();
    info!("primary_init: vm::init() done, calling drivers::init()...");
    drivers::init();

    // Initialize executor runtimes for all CPUs.
    // On aarch64, CPU IDs are contiguous 0..N.
    let cpu_ids: alloc::vec::Vec<u8> = (0..crate::config::MAX_CORE_NUM as u8).collect();
    executor::init_runtimes(&cpu_ids);

    // Start secondary cores (QEMU virt uses PSCI).
    // UEFI boot and Jolla C2: SMP not yet implemented (single-core for now).
    #[cfg(all(
        not(feature = "board-raspi400"),
        not(feature = "board-jollac2"),
        not(feature = "uefi-boot")
    ))]
    start_secondary_cores();
}

/// Start secondary cores via PSCI CPU_ON.
///
/// On QEMU virt, secondary cores are powered off at boot. We use
/// PSCI CPU_ON to start each one at the `_secondary_entry` physical
/// address. The secondary entry assembly enables the MMU and jumps
/// to `secondary_core_init` in Rust.
#[cfg(all(
    not(feature = "board-raspi400"),
    not(feature = "board-jollac2"),
    not(feature = "uefi-boot")
))]
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
    #[cfg(feature = "board-jollac2")]
    {
        // Virtual timer PPI 11 = IRQ 27 (same as Pi 400)
        27
    }
    #[cfg(all(not(feature = "board-raspi400"), not(feature = "board-jollac2")))]
    {
        30
    }
}

pub fn timer_init() {
    timer::init();
}
pub mod platform;

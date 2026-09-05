// x86_64 entry point using the bootloader crate.
//
// The bootloader handles UEFI/BIOS boot, page table setup, and
// provides boot info to the kernel via bootloader_api::BootInfo.

use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::info::{MemoryRegionKind, Optional};
use kernel_hal::config::{FramebufferInfo, KernelConfig, MemoryRegion, MemoryType};

/// Maximum number of memory regions we can store.
const MAX_MEMORY_REGIONS: usize = 256;

/// Static storage for converted memory regions.
static mut MEMORY_REGIONS: [MemoryRegion; MAX_MEMORY_REGIONS] = [MemoryRegion {
    phys_start: 0,
    page_count: 0,
    memory_type: MemoryType::Reserved,
}; MAX_MEMORY_REGIONS];

static mut MEMORY_REGION_COUNT: usize = 0;

/// Bootloader configuration.
///
/// All bootloader-managed regions must be in the upper half of virtual address
/// space (PML4 entries 0x100..0x200) because `pt_clone_kernel_space` only
/// copies those entries into user process page tables. Any mapping in the
/// lower half becomes inaccessible when a user page table is active.
/// Max ramdisk size: the gap between ramdisk_memory and boot_info (255 MiB).
const RAMDISK_MAX_SIZE: u64 = 0xFFFF_FFFF_7FF0_0000 - 0xFFFF_FFFF_7000_0000;

const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::FixedAddress(0xFFFF_8000_0000_0000));
    config.mappings.kernel_stack = Mapping::FixedAddress(0xFFFF_FFFF_7FFE_0000);
    config.mappings.boot_info = Mapping::FixedAddress(0xFFFF_FFFF_7FF0_0000);
    config.mappings.ramdisk_memory = Mapping::FixedAddress(0xFFFF_FFFF_7000_0000);
    config.mappings.framebuffer = Mapping::FixedAddress(0xFFFF_FFFF_6000_0000);
    config
};

// Define the bootloader entry point with our config
bootloader_api::entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // Guard against oversized ramdisks that would overlap the boot_info region.
    assert!(
        boot_info.ramdisk_len <= RAMDISK_MAX_SIZE,
        "ramdisk too large ({} bytes, max {}): would overlap boot_info mapping",
        boot_info.ramdisk_len,
        RAMDISK_MAX_SIZE,
    );

    // Convert bootloader memory regions to our kernel-owned type
    let count = boot_info.memory_regions.len().min(MAX_MEMORY_REGIONS);
    unsafe {
        for (i, r) in boot_info.memory_regions.iter().take(count).enumerate() {
            *core::ptr::addr_of_mut!(MEMORY_REGIONS[i]) = MemoryRegion {
                phys_start: r.start,
                page_count: (r.end - r.start) / 4096,
                memory_type: match r.kind {
                    MemoryRegionKind::Usable => MemoryType::Conventional,
                    MemoryRegionKind::Bootloader => MemoryType::BootServicesData,
                    _ => MemoryType::Reserved,
                },
            };
        }
        *core::ptr::addr_of_mut!(MEMORY_REGION_COUNT) = count;
    }

    let framebuffer = match &boot_info.framebuffer {
        Optional::Some(fb) => {
            let info = fb.info();
            // The buffer pointer is a virtual address. To get the physical address,
            // subtract the physical_memory_offset. If offset is not available,
            // use the virtual address (it will still work for MMIO framebuffers
            // since the bootloader maps them identity or at the phys offset).
            let vaddr = fb.buffer().as_ptr() as u64;
            let phys_addr = match boot_info.physical_memory_offset {
                Optional::Some(offset) => vaddr - offset,
                Optional::None => vaddr,
            };
            Some(FramebufferInfo {
                width: info.width as u32,
                height: info.height as u32,
                stride: info.stride as u32,
                addr: phys_addr,
                size: fb.buffer().len() as u64,
            })
        }
        Optional::None => None,
    };

    let phys_offset = match boot_info.physical_memory_offset {
        Optional::Some(offset) => offset as usize,
        Optional::None => 0,
    };

    let rsdp = match boot_info.rsdp_addr {
        Optional::Some(addr) => addr,
        Optional::None => 0,
    };

    let config = KernelConfig {
        cmdline: option_env!("ZCORE_CMDLINE").unwrap_or("LOG=warn"),
        initrd_start: match boot_info.ramdisk_addr {
            Optional::Some(addr) => addr,
            Optional::None => 0,
        },
        initrd_size: boot_info.ramdisk_len,
        memory_map: unsafe {
            core::slice::from_raw_parts(
                core::ptr::addr_of!(MEMORY_REGIONS) as *const MemoryRegion,
                *core::ptr::addr_of!(MEMORY_REGION_COUNT),
            )
        },
        phys_to_virt_offset: phys_offset,
        framebuffer,
        acpi_rsdp: rsdp,
        smbios: 0,
        ap_fn: crate::secondary_main,
    };

    crate::primary_main(config);
    unreachable!()
}

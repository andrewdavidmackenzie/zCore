//! Kernel configuration for x86_64.
//!
//! Types for boot-time data (memory map, framebuffer) and statics
//! for arch-specific values that don't belong in the unified KernelConfig.

use crate::utils::init_once::InitOnce;

/// A memory region reported by the bootloader.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryRegion {
    /// Physical start address.
    pub phys_start: u64,
    /// Number of 4 KiB pages.
    pub page_count: u64,
    /// Memory type.
    pub memory_type: MemoryType,
}

/// Memory region type (simplified from UEFI memory types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryType {
    /// Usable conventional memory.
    Conventional = 7,
    /// Memory used by the bootloader (reclaimable).
    BootServicesData = 4,
    /// Memory used by the bootloader code (reclaimable).
    BootServicesCode = 3,
    /// Reserved / unusable.
    Reserved = 0,
    /// Other types (ACPI, MMIO, etc.).
    Other = 0xFF,
}

/// Framebuffer information from the bootloader.
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Horizontal resolution in pixels.
    pub width: u32,
    /// Vertical resolution in pixels.
    pub height: u32,
    /// Stride in pixels (pixels per scan line).
    pub stride: u32,
    /// Physical address of the framebuffer.
    pub addr: u64,
    /// Size of the framebuffer in bytes.
    pub size: u64,
}

// --- Arch-specific statics (not in the unified KernelConfig) ---

/// Framebuffer info, set by the platform entry point.
pub(crate) static FRAMEBUFFER: InitOnce<Option<FramebufferInfo>> = InitOnce::new_with_default(None);

/// SMBIOS address, set by the platform entry point.
pub(crate) static SMBIOS: InitOnce<u64> = InitOnce::new_with_default(0);

/// Memory map from the bootloader.
pub(crate) static MEMORY_MAP: InitOnce<&'static [MemoryRegion]> = InitOnce::new_with_default(&[]);

/// Function to start on Application Processor cores.
pub(crate) static AP_FN: InitOnce<Option<fn() -> !>> = InitOnce::new_with_default(None);

/// Set arch-specific boot data. Called from the platform entry point
/// before `primary_init_early()`.
pub fn set_x86_boot_data(
    framebuffer: Option<FramebufferInfo>,
    smbios: u64,
    memory_map: &'static [MemoryRegion],
    ap_fn: Option<fn() -> !>,
) {
    FRAMEBUFFER.init_once_by(framebuffer);
    SMBIOS.init_once_by(smbios);
    MEMORY_MAP.init_once_by(memory_map);
    AP_FN.init_once_by(ap_fn);
}

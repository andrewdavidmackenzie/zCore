//! Kernel configuration for x86_64.

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

/// Kernel configuration passed by the bootloader.
#[derive(Debug)]
pub struct KernelConfig {
    pub cmdline: &'static str,
    pub initrd_start: u64,
    pub initrd_size: u64,

    pub memory_map: &'static [MemoryRegion],
    pub phys_to_virt_offset: usize,

    pub framebuffer: Option<FramebufferInfo>,

    pub acpi_rsdp: u64,
    pub smbios: u64,
    pub ap_fn: fn() -> !,
}

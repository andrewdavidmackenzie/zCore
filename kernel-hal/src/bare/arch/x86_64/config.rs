//! Kernel configuration for x86_64.
//!
//! On x86_64, the bootloader provides a BootInfo struct that is passed
//! to the kernel entry point. The KernelConfig stores a reference to it.

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
    /// Kernel command line (currently empty -- bootloader doesn't provide one).
    pub cmdline: &'static str,
    /// Initramfs/ramdisk start address.
    pub initrd_start: u64,
    /// Initramfs/ramdisk size.
    pub initrd_size: u64,

    /// Memory map from the bootloader.
    pub memory_map: &'static [MemoryRegion],
    /// Offset added to physical addresses to get virtual addresses.
    pub phys_to_virt_offset: usize,

    /// Framebuffer for display output.
    pub framebuffer: Option<FramebufferInfo>,

    /// ACPI RSDP physical address (0 if not available).
    pub acpi_rsdp: u64,
    /// SMBIOS address (0 if not available).
    pub smbios: u64,
    /// Function to start on Application Processor cores.
    pub ap_fn: fn() -> !,
}

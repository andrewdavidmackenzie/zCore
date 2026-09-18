//! Unified kernel configuration passed from platform boot code to the kernel.
//!
//! Each platform entry point fills in the fields it knows about.
//! Fields not relevant to a platform are left as their defaults (0, None, "").

/// Kernel configuration provided by the platform boot code.
///
/// This is a flat struct with all fields that any platform might provide.
/// Platform entry points populate only the fields they know about.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Kernel command line (e.g., "LOG=info ROOTPROC=/bin/sh").
    pub cmdline: &'static str,

    /// Offset from physical to virtual addresses.
    /// Added to a physical address to get the corresponding kernel virtual address.
    /// 0 for libos (no physical/virtual distinction).
    pub phys_to_virt_offset: usize,

    /// DTB (Device Tree Blob) physical address. 0 if not available.
    pub dtb_paddr: usize,
    /// DTB size in bytes. 0 if not available.
    pub dtb_size: usize,

    /// Initramfs/ramdisk start physical address. 0 if not available.
    pub initrd_start: u64,
    /// Initramfs/ramdisk size in bytes. 0 if not available.
    pub initrd_size: u64,

    /// ACPI RSDP physical address. 0 if not available (non-x86 or no ACPI).
    pub acpi_rsdp: u64,

    /// Function to call on secondary (AP) cores. Only used on x86_64 SMP.
    /// None on single-core platforms or platforms that start APs differently.
    pub ap_fn: Option<fn() -> !>,
}

impl KernelConfig {
    /// Const constructor with all fields zeroed/None/"".
    pub const fn new() -> Self {
        Self {
            cmdline: "",
            phys_to_virt_offset: 0,
            dtb_paddr: 0,
            dtb_size: 0,
            initrd_start: 0,
            initrd_size: 0,
            acpi_rsdp: 0,
            ap_fn: None,
        }
    }
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self::new()
    }
}

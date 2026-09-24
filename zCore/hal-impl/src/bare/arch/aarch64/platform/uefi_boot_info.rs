/// Boot info passed from the UEFI stub to the kernel.
///
/// The stub places this struct at a known physical address and passes
/// the address in x0 to `rust_main_uefi`. Both the stub and kernel
/// must agree on this layout.
#[repr(C)]
pub struct UefiBootInfo {
    /// Magic number to validate the struct (0x5A_43_55_45 = "ZCUE")
    pub magic: u64,
    /// DTB physical address (0 if not available)
    pub dtb_paddr: u64,
    /// DTB size in bytes
    pub dtb_size: u64,
    /// Initrd physical address (0 if not available)
    pub initrd_start: u64,
    /// Initrd size in bytes
    pub initrd_size: u64,
}

impl UefiBootInfo {
    pub const MAGIC: u64 = 0x5A_43_55_45; // "ZCUE"
}

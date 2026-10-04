//! Kernel configuration.
use crate::PAGE_SIZE;

// --- Board-specific constants ---

#[cfg(all(not(feature = "board-raspi400"), not(feature = "board-jollac2")))]
mod board_config {
    /// QEMU virt: RAM starts at 1 GiB
    pub const PHYS_MEMORY_BASE: usize = 0x4000_0000;
    pub const PHYS_MEMORY_END: usize = PHYS_MEMORY_BASE + 100 * 1024 * 1024;
    pub const VIRTIO_BASE: usize = 0x0a00_0000;
    pub const VIRTIO_SIZE: usize = 0x100;
}

#[cfg(feature = "board-raspi400")]
mod board_config {
    /// RPi 400: RAM starts at 0, 4 GiB total.
    /// For raw boot the kernel is at 0x80000, for UEFI boot at 0x80200000.
    /// PHYS_MEMORY_END must cover both cases — use 1 GiB as a safe
    /// fallback when DTB doesn't report memory size.
    pub const PHYS_MEMORY_BASE: usize = 0x0000_0000;
    pub const PHYS_MEMORY_END: usize = 0xFC00_0000; // ~4 GiB fallback (below peripherals)
    /// RPi 400 has no VirtIO -- use dummy values (never mapped)
    pub const VIRTIO_BASE: usize = 0;
    pub const VIRTIO_SIZE: usize = 0;
}

#[cfg(feature = "board-jollac2")]
mod board_config {
    /// Jolla C2 / UMS9230: RAM starts at 0x80000000, 6 GiB total.
    /// Kernel loaded at 0x80080000.
    /// Cap below first reserved-memory region (WCNSS SIPC at 0x87240000).
    /// DTB reserved-memory parsing is not yet implemented, so we use a
    /// conservative static limit to avoid allocating over firmware regions.
    pub const PHYS_MEMORY_BASE: usize = 0x8000_0000;
    pub const PHYS_MEMORY_END: usize = 0x8700_0000;
    /// No VirtIO on Jolla C2
    pub const VIRTIO_BASE: usize = 0;
    pub const VIRTIO_SIZE: usize = 0;
    /// Framebuffer physical base (display controller DMA buffer).
    pub const FB_PHYS: usize = 0x9e00_0000;
    /// Framebuffer size: 720 * 1600 * 4 (ARGB8888) = 4,608,000 bytes.
    /// Round up to page boundary (1126 pages = 0x466000).
    pub const FB_SIZE: usize = 720 * 1600 * 4;
}

pub use board_config::*;

pub const UART_SIZE: usize = 0x1000;
pub const PA_1TB_BITS: usize = 40;
pub const PHYS_ADDR_MAX: usize = (1 << PA_1TB_BITS) - 1;
pub const PHYS_ADDR_MASK: usize = PHYS_ADDR_MAX & !(PAGE_SIZE - 1);
pub const USER_TABLE_FLAG: usize = 0xabcd_0000_0000_0000;

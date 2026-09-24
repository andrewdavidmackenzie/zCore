//! Kernel configuration.
use crate::PAGE_SIZE;

// --- Board-specific constants ---

#[cfg(not(feature = "board-raspi400"))]
mod board_config {
    /// QEMU virt: RAM starts at 1 GiB.
    pub const PHYS_MEMORY_BASE: usize = 0x4000_0000;
    /// Fallback memory end if DTB doesn't report memory size.
    /// Set to 2 GiB from base (covers standard QEMU virt config).
    pub const PHYS_MEMORY_END: usize = PHYS_MEMORY_BASE + 2 * 1024 * 1024 * 1024;
    pub const VIRTIO_BASE: usize = 0x0a00_0000;
    pub const VIRTIO_SIZE: usize = 0x100;
}

#[cfg(feature = "board-raspi400")]
mod board_config {
    /// RPi 400: RAM starts at 0
    pub const PHYS_MEMORY_BASE: usize = 0x0000_0000;
    pub const PHYS_MEMORY_END: usize = PHYS_MEMORY_BASE + 100 * 1024 * 1024;
    /// RPi 400 has no VirtIO -- use dummy values (never mapped)
    pub const VIRTIO_BASE: usize = 0;
    pub const VIRTIO_SIZE: usize = 0;
}

pub use board_config::*;

pub const UART_SIZE: usize = 0x1000;
pub const PA_1TB_BITS: usize = 40;
pub const PHYS_ADDR_MAX: usize = (1 << PA_1TB_BITS) - 1;
pub const PHYS_ADDR_MASK: usize = PHYS_ADDR_MAX & !(PAGE_SIZE - 1);
pub const USER_TABLE_FLAG: usize = 0xabcd_0000_0000_0000;

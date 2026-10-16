//! Common HAL type definitions.

use bitflags::bitflags;

/// Page size constant.
///
/// On aarch64 macOS (libos mode), use 16 KiB to match the host page
/// size. This eliminates the 4K/16K mismatch that causes mock_mem
/// to need anonymous mapping fallbacks (which fail with EACCES on
/// macOS when address ranges have stale file associations).
///
/// Newer ARM hardware (e.g., Raspberry Pi 5) can also use 16K pages,
/// so this exercises zCore's 16K page support.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub const PAGE_SIZE: usize = 16384;

/// Page size constant (4 KiB) — default for all other platforms.
#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub const PAGE_SIZE: usize = 4096;

bitflags! {
    /// Generic memory flags.
    pub struct MMUFlags: usize {
        #[allow(clippy::identity_op)]
        const CACHE_1   = 1 << 0;
        const CACHE_2   = 1 << 1;
        const READ      = 1 << 2;
        const WRITE     = 1 << 3;
        const EXECUTE   = 1 << 4;
        const USER      = 1 << 5;
        const HUGE_PAGE = 1 << 6;
        const DEVICE    = 1 << 7;
        const RXW = Self::READ.bits | Self::WRITE.bits | Self::EXECUTE.bits;
    }
}

/// Generic cache policy.
#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CachePolicy {
    Cached = 0,
    Uncached = 1,
    UncachedDevice = 2,
    WriteCombining = 3,
}

impl TryFrom<u32> for CachePolicy {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Cached),
            1 => Ok(Self::Uncached),
            2 => Ok(Self::UncachedDevice),
            3 => Ok(Self::WriteCombining),
            other => Err(other),
        }
    }
}

impl From<CachePolicy> for u32 {
    fn from(val: CachePolicy) -> u32 {
        val as u32
    }
}

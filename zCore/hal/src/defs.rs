//! Common HAL type definitions.

use bitflags::bitflags;

/// Page size constant (4 KiB).
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

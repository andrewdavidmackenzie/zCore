//! Definition of physical, virtual addresses and helper functions.

use core::fmt::{LowerHex, UpperHex};

use crate::PAGE_SIZE;

/// Physical address.
///
/// A newtype over `usize` so physical and virtual addresses cannot be
/// confused at function boundaries: passing a virtual address where a
/// `PhysAddr` is expected is a type error, and the reverse direction
/// requires an explicit conversion. `VirtAddr` remains a plain
/// `usize` alias (the untyped default), which is sufficient: with
/// `PhysAddr` typed, both mixup directions are caught.
///
/// `#[repr(transparent)]` keeps FFI/inline-asm compatibility, and the
/// public field keeps arithmetic call sites mechanical (`paddr.0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct PhysAddr(pub usize);

impl PhysAddr {
    /// The null address.
    pub const ZERO: PhysAddr = PhysAddr(0);

    /// Construct a physical address from a raw value.
    pub const fn new(addr: usize) -> Self {
        PhysAddr(addr)
    }

    /// The raw value.
    pub const fn data(self) -> usize {
        self.0
    }

    /// Whether the address is page-aligned.
    pub const fn is_aligned(self) -> bool {
        self.0 & (PAGE_SIZE - 1) == 0
    }

    /// Round down to the page boundary.
    pub const fn align_down(self) -> Self {
        PhysAddr(self.0 & !(PAGE_SIZE - 1))
    }

    /// The offset within the page.
    pub const fn page_offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }
}

impl From<usize> for PhysAddr {
    fn from(addr: usize) -> Self {
        PhysAddr(addr)
    }
}

impl From<PhysAddr> for usize {
    fn from(addr: PhysAddr) -> Self {
        addr.0
    }
}

impl core::fmt::LowerHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(&self.0, f)
    }
}

impl core::fmt::UpperHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        UpperHex::fmt(&self.0, f)
    }
}

impl core::ops::Add<usize> for PhysAddr {
    type Output = Self;
    fn add(self, rhs: usize) -> Self {
        PhysAddr(self.0 + rhs)
    }
}

impl core::ops::AddAssign<usize> for PhysAddr {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl core::ops::Sub<usize> for PhysAddr {
    type Output = Self;
    fn sub(self, rhs: usize) -> Self {
        PhysAddr(self.0 - rhs)
    }
}

impl core::ops::SubAssign<usize> for PhysAddr {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

/// The difference of two physical addresses, in bytes.
impl core::ops::Sub for PhysAddr {
    type Output = usize;
    fn sub(self, rhs: Self) -> usize {
        self.0 - rhs.0
    }
}

/// Virtual address.
///
/// Deliberately a plain `usize` alias for now (see [`PhysAddr`]); a
/// follow-up may type it symmetrically.
pub type VirtAddr = usize;

/// Device address.
pub type DevVAddr = usize;

pub const fn align_down(addr: usize) -> usize {
    addr & !(PAGE_SIZE - 1)
}

/// Round `addr` up to the next page boundary.
///
/// Returns 0 for values above `usize::MAX - PAGE_SIZE + 1` (wraps to 0
/// rather than panicking), matching the behavior expected by callers
/// that check the result.
pub const fn align_up(addr: usize) -> usize {
    match addr.checked_add(PAGE_SIZE - 1) {
        Some(v) => v & !(PAGE_SIZE - 1),
        None => 0, // overflow: not representable
    }
}

pub const fn is_aligned(addr: usize) -> bool {
    page_offset(addr) == 0
}

pub const fn page_count(size: usize) -> usize {
    align_up(size) / PAGE_SIZE
}

pub const fn page_offset(addr: usize) -> usize {
    addr & (PAGE_SIZE - 1)
}

/// Whether a physical address is page-aligned.
pub const fn paddr_is_aligned(paddr: PhysAddr) -> bool {
    paddr.is_aligned()
}

//! Definition of physical, virtual addresses and helper functions.

use crate::PAGE_SIZE;

/// Physical address.
pub type PhysAddr = usize;

/// Virtual address.
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

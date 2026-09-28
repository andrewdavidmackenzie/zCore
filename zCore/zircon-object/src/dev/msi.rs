//! MSI (Message Signaled Interrupt) allocation kernel object.

use crate::object::*;
use alloc::sync::Arc;
use lock::Mutex;

/// An MSI allocation represents a block of contiguous MSI interrupt
/// vectors allocated from the platform MSI controller.
pub struct MsiAllocation {
    base: KObjectBase,
    /// Number of vectors allocated.
    count: u32,
    /// Base IRQ number (0 if not backed by real hardware).
    base_irq: usize,
    /// Which vectors have been used to create Interrupt objects.
    used: Mutex<u32>,
}

impl_kobject!(MsiAllocation);

impl MsiAllocation {
    /// Create a new MSI allocation with the given vector count.
    ///
    /// `count` must be a power of two in [1, 32].
    /// `base_irq` is the first IRQ number in the allocated block
    /// (0 if no real hardware backend is available).
    pub fn create(count: u32, base_irq: usize) -> Arc<Self> {
        Arc::new(Self {
            base: KObjectBase::new(),
            count,
            base_irq,
            used: Mutex::new(0),
        })
    }

    /// Number of vectors in this allocation.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Base IRQ number.
    pub fn base_irq(&self) -> usize {
        self.base_irq
    }

    /// Mark a vector as used. Returns the IRQ number for the vector.
    /// Returns `ALREADY_EXISTS` if the vector was already used.
    pub fn use_vector(&self, msi_id: u32) -> ZxResult<usize> {
        if msi_id >= self.count {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let mut used = self.used.lock();
        let mask = 1u32 << msi_id;
        if *used & mask != 0 {
            return Err(ZxError::ALREADY_EXISTS);
        }
        *used |= mask;
        Ok(self.base_irq + msi_id as usize)
    }
}

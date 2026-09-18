//! Block device driver trait.

use super::Scheme;
use crate::DeviceResult;

/// Trait for block device drivers.
pub trait BlockScheme: Scheme {
    /// Read one block into `buf`.
    fn read_block(&self, block_id: usize, buf: &mut [u8]) -> DeviceResult;

    /// Write one block from `buf`.
    fn write_block(&self, block_id: usize, buf: &[u8]) -> DeviceResult;

    /// Flush pending writes.
    fn flush(&self) -> DeviceResult;
}

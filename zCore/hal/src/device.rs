//! Device and HAL error types.

/// The unified error type for device and HAL operations.
#[derive(Debug)]
pub enum DeviceError {
    /// The buffer is too small.
    BufferTooSmall,
    /// The device is not ready.
    NotReady,
    /// Invalid parameter.
    InvalidParam,
    /// Failed to alloc DMA memory.
    DmaError,
    /// I/O Error
    IoError,
    /// A resource with the specified identifier already exists.
    AlreadyExists,
    /// No resource to allocate.
    NoResources,
    /// The device driver is not implemented, supported, or enabled.
    NotSupported,
}

/// A type alias for the result of a device operation.
pub type DeviceResult<T = ()> = core::result::Result<T, DeviceError>;

//! Handlers implemented in kernel and called by HAL.

use crate::utils::init_once::InitOnce;

// Re-export the KernelHandler trait from the hal crate.
pub use hal::KernelHandler;

#[allow(dead_code)]
pub(crate) struct DummyKernelHandler;

#[cfg(feature = "libos")]
pub(crate) static KHANDLER: InitOnce<&dyn KernelHandler> =
    InitOnce::new_with_default(&DummyKernelHandler);

#[cfg(not(feature = "libos"))]
pub(crate) static KHANDLER: InitOnce<&dyn KernelHandler> = InitOnce::new();

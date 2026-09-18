use crate::utils::init_once::InitOnce;

// Re-export the unified KernelConfig from the hal crate.
pub use hal::KernelConfig;

#[cfg(feature = "libos")]
pub(crate) static KCONFIG: InitOnce<KernelConfig> = InitOnce::new_with_default(KernelConfig::new());

#[cfg(not(feature = "libos"))]
pub(crate) static KCONFIG: InitOnce<KernelConfig> = InitOnce::new();

pub const MAX_CORE_NUM: usize = 8;

// Re-export arch-specific config types and functions so that
// `hal_impl::config::FramebufferInfo` etc. work from entry points.
#[cfg(all(target_arch = "x86_64", not(feature = "libos")))]
pub use crate::imp::config::{set_x86_boot_data, FramebufferInfo, MemoryRegion, MemoryType};

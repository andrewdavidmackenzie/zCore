use crate::utils::init_once::InitOnce;

// Re-export the unified KernelConfig from the hal crate.
pub use hal::KernelConfig;

#[cfg(feature = "libos")]
pub(crate) static KCONFIG: InitOnce<KernelConfig> = InitOnce::new_with_default(KernelConfig::new());

#[cfg(not(feature = "libos"))]
pub(crate) static KCONFIG: InitOnce<KernelConfig> = InitOnce::new();

/// Maximum number of CPU cores, set by the target config's `cores` field
/// via the `ZCORE_MAX_CPUS` environment variable at build time.
pub const MAX_CORE_NUM: usize = {
    // Parse the build-time constant from build.rs
    // env! is evaluated at compile time from cargo:rustc-env
    const VAL: &str = env!("MAX_CPUS");
    // const-parse a decimal string to usize
    let bytes = VAL.as_bytes();
    let mut result: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        result = result * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    result
};

// Re-export arch-specific config types and functions so that
// `hal_impl::config::FramebufferInfo` etc. work from entry points.
#[cfg(all(target_arch = "x86_64", not(feature = "libos")))]
pub use crate::imp::config::{set_x86_boot_data, FramebufferInfo, MemoryRegion, MemoryType};

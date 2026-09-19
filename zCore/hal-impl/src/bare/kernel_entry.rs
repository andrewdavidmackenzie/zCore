//! Extern declarations for kernel entry points.
//!
//! These functions are defined in the kernel binary crate (main.rs)
//! with `#[no_mangle] pub extern "Rust"` and resolved by the linker.

use hal::KernelConfig;

extern "Rust" {
    /// Primary boot entry point. Called once by the BSP (bootstrap processor).
    pub fn primary_core_init(config: KernelConfig) -> !;

    /// Secondary core initialization. Called by each AP (application processor).
    pub fn secondary_core_init() -> !;
}

mod drivers;
mod dummy;
mod mock_mem;
pub mod platform;

pub mod boot;
pub mod config;
pub mod cpu;
pub mod interrupt;
pub mod mem;
// net module removed: loopback/network features moved out of kernel (#237)
pub mod thread;
pub mod timer;
pub mod vdso;
pub mod vm;

#[path = "special.rs"]
pub mod libos;

pub use super::hal_fn::rand;

hal_fn_impl_default!(rand);

// Provide a real console for libos mode -- write to host stderr.
hal_fn_impl! {
    impl mod crate::hal_fn::console {
        fn console_write_early(s: &str) {
            use std::io::Write;
            let _ = std::io::stderr().write_all(s.as_bytes());
        }
    }
}
// platform module is implemented in platform.rs, not default

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod macos;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod aarch64_macos_fncall;

/// Non-SMP initialization.
pub fn init() {
    drivers::init_early();
    boot::primary_init();
}

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[path = "arch/x86_64/mod.rs"]
        pub(crate) mod arch;
        pub use self::arch::{special as x86_64, timer_interrupt_vector};
    } else if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        #[path = "arch/riscv/mod.rs"]
        pub mod arch;
        pub use self::arch::{sbi, timer_interrupt_vector};
    } else if #[cfg(target_arch = "aarch64")] {
        #[path = "arch/aarch64/mod.rs"]
        pub mod arch;
        pub use self::arch::timer_interrupt_vector;
    }
}

pub mod boot;
pub(crate) mod kernel_entry;
pub mod lang;
pub mod mem;
pub mod memory;
mod run_executor;
pub use run_executor::run_executor;
// net module removed: loopback/network features moved out of kernel (#237)
pub mod thread;
pub mod timer;

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub use self::arch::config;
pub use self::arch::{cpu, interrupt, vm};
pub use super::hal_fn::{platform, rand, vdso};

hal_fn_impl_default!(rand, vdso, platform);

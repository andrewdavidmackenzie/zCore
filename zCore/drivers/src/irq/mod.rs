//! Interrupt controller drivers.
//!
//! Each controller is gated by its own feature flag.

#[cfg(feature = "riscv-intc")]
mod riscv_intc;
#[cfg(feature = "riscv-plic")]
mod riscv_plic;

/// RISC-V interrupt controller implementations.
#[cfg(any(feature = "riscv-intc", feature = "riscv-plic"))]
pub mod riscv {
    #[cfg(feature = "riscv-intc")]
    pub use super::riscv_intc::{Intc, ScauseIntCode};
    #[cfg(feature = "riscv-plic")]
    pub use super::riscv_plic::Plic;
}

#[cfg(feature = "apic")]
mod x86_apic;
/// x86 Advanced Programmable Interrupt Controller.
#[cfg(feature = "apic")]
pub mod x86 {
    pub use super::x86_apic::Apic;
}

/// ARM Generic Interrupt Controller (GIC-400).
#[cfg(feature = "gic-400")]
pub mod gic_400;

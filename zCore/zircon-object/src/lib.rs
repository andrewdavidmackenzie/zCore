//! Zircon kernel objects
//!
//! # Feature flags
//!
//! - `elf`: Enables `zircon_object::util::elf_loader`.
//! - `hypervisor`: Enables `zircon_object::hypervisor` (`Guest` and `Vcpu`).

#![cfg_attr(not(test), no_std)]
#![deny(warnings)]
// The `verus_only` items in signal/futex.rs (opaque type specs for the
// spin lock) reference the unstable `sized_hierarchy` trait bounds that
// Verus's rustc emits for generic lock types. Gated: only the Verus
// build (which sets RUSTC_BOOTSTRAP) sees it; normal builds never do.
#![cfg_attr(verus_only, feature(sized_hierarchy))]
// The `impl_downcast!` macro from `downcast-rs` triggers this lint; we cannot fix upstream.
#![allow(clippy::multiple_bound_locations)]
// #![deny(missing_docs)] // effectively unenforced

extern crate alloc;

#[macro_use]
extern crate log;

// Formal verification support (#833): `verus!` blocks in this crate
// (see signal/futex.rs) expand to plain Rust under normal rustc — specs
// and proofs are erased — and are machine-checked by `make verify`
// (cargo-verus). This import brings in the `verus!` macro and spec
// types; ghost-only imports inside modules use `#[cfg(verus_only)]`.
use vstd::prelude::*;

pub mod debuglog;
pub mod dev;
mod error;
#[cfg(feature = "hypervisor")]
pub mod hypervisor;
pub mod ipc;
pub mod mutex;
pub mod object;
pub mod signal;
pub mod task;
pub mod util;
pub mod vm;

pub use self::error::*;

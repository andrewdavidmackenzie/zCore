//! Linux kernel objects

#![no_std]
#![deny(warnings)]
// #![deny(missing_docs)] // effectively unenforced
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::uninit_vec)]
#![allow(clippy::multiple_bound_locations)]
#![allow(clippy::double_must_use)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

// layer 0
pub mod error;

// layer 1
pub mod fs;

// layer 2
pub mod ipc;
pub mod loader;
// net module requires smoltcp and kernel_hal::net which were removed (#237).
// Gate behind a feature to avoid compilation errors.
#[cfg(feature = "net")]
pub mod net;
pub mod process;
pub mod signal;
pub mod sync;
pub mod thread;
pub mod time;

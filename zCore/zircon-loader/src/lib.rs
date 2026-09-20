//! Zircon user program loader and runner.

#![no_std]
#![deny(warnings, missing_docs)]

extern crate alloc;
#[macro_use]
extern crate log;

pub mod zircon;

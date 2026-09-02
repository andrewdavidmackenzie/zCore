//! Zircon user program loader and runner.

#![no_std]
#![deny(warnings, missing_docs)]

extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate cfg_if;

pub mod zircon;

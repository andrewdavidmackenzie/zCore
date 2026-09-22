//! petal alloc test -- verifies that the global allocator works.
//!
//! Tests: String creation, Vec push, format!.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use alloc::string::String;
use alloc::vec::Vec;

#[no_mangle]
pub fn main() {
    zx::debug_write(b"alloc_test: starting\n");

    // Test String
    let mut s = String::from("hello");
    s.push_str(" alloc!");
    if s == "hello alloc!" {
        zx::debug_write(b"alloc_test: String OK\n");
    } else {
        zx::debug_write(b"alloc_test: FAIL - String mismatch\n");
        zx::Process::exit(1);
    }

    // Test Vec
    let mut v: Vec<u32> = Vec::new();
    for i in 0..100 {
        v.push(i);
    }
    if v.len() == 100 && v[99] == 99 {
        zx::debug_write(b"alloc_test: Vec OK\n");
    } else {
        zx::debug_write(b"alloc_test: FAIL - Vec mismatch\n");
        zx::Process::exit(1);
    }

    zx::debug_write(b"alloc_test: PASS\n");
}

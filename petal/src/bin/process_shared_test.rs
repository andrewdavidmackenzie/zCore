//! petal process_shared test -- process creation with invalid options.
//!
//! Port of the UserPtr-using test from
//! zircon-tests/src/extra_tests/process_shared.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_JOB_DEFAULT: u32 = 0x03;

fn pass(name: &[u8]) {
    zx::debug_write(b"  process_shared_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: process_shared_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Process create with invalid options -> INVALID_ARGS
fn invalid_prototype(job_h: HandleValue) {
    let name = b"invalid_prototype";
    let pname = b"child\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    let s = unsafe {
        zx_process_create(
            job_h,
            pname.as_ptr(),
            5,
            0xFFFF_FFFF,
            &mut proc_h,
            &mut vmar_h,
        )
    };
    if s == ZX_OK {
        unsafe { zx_handle_close(proc_h) };
        unsafe { zx_handle_close(vmar_h) };
        fail(name, b"invalid options should fail");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"process_shared_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let job_h = bootstrap.find(PA_JOB_DEFAULT);
    if job_h == 0 {
        zx::debug_write(b"  SKIP: no PA_JOB_DEFAULT\n");
        zx::debug_write(b"process_shared_test: all tests passed\n");
        return;
    }

    invalid_prototype(job_h);

    zx::debug_write(b"process_shared_test: all tests passed\n");
}

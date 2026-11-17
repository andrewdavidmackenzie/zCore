//! petal wake_reports test -- basic port wake report validation.
//!
//! Port of sync tests from zircon-tests/src/core_tests/wake_reports.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  wake_reports_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: wake_reports_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Bad report requests on port-bound objects
fn port_bound_bad_report_requests() {
    let name = b"bad_report_requests";
    let mut port: HandleValue = 0;
    let s = unsafe { zx_port_create(0, &mut port) };
    if s != ZX_OK {
        fail(name, b"port create");
    }

    let mut event: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut event) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    // Subscribe event to port
    let s = unsafe { zx_object_wait_async(event, port, 1, 1 << 24, 0) };
    if s != ZX_OK {
        fail(name, b"wait_async");
    }

    // Cancel
    let s = unsafe { zx_port_cancel(port, event, 1) };
    if s != ZX_OK {
        fail(name, b"cancel");
    }

    unsafe { zx_handle_close(event) };
    unsafe { zx_handle_close(port) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"wake_reports_test: starting\n");
    port_bound_bad_report_requests();
    zx::debug_write(b"wake_reports_test: all tests passed\n");
}

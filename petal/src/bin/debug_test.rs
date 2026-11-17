//! petal debug test -- exercises debug_write and debug_send_command.
//!
//! Port of zircon-tests/src/debug.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;

fn pass(name: &[u8]) {
    zx::debug_write(b"  debug_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: debug_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

/// debug_write with valid data
fn debug_write_basic() {
    let name = b"debug_write_basic";
    let data = b"debug_write test\n";
    let s = unsafe { zx_debug_write(data.as_ptr(), data.len()) };
    assert_ok(s, name, b"");
    pass(name);
}

/// debug_write with zero length
fn debug_write_empty() {
    let name = b"debug_write_empty";
    let s = unsafe { zx_debug_write(core::ptr::null(), 0) };
    assert_ok(s, name, b"");
    pass(name);
}

/// debug_write oversized (> 4096)
fn debug_write_too_large() {
    let name = b"debug_write_too_large";
    let s = unsafe { zx_debug_write(core::ptr::null(), 5000) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"");
    pass(name);
}

/// debug_send_command "help"
fn debug_send_command_help(resource: HandleValue) {
    let name = b"debug_send_command_help";
    let cmd = b"help";
    let s = unsafe { zx_debug_send_command(resource, cmd.as_ptr(), cmd.len()) };
    assert_ok(s, name, b"");
    pass(name);
}

/// debug_send_command "ps"
fn debug_send_command_ps(resource: HandleValue) {
    let name = b"debug_send_command_ps";
    let cmd = b"ps";
    let s = unsafe { zx_debug_send_command(resource, cmd.as_ptr(), cmd.len()) };
    assert_ok(s, name, b"");
    pass(name);
}

/// debug_send_command unknown command
fn debug_send_command_unknown(resource: HandleValue) {
    let name = b"debug_send_command_unknown";
    let cmd = b"nonexistent_command";
    let s = unsafe { zx_debug_send_command(resource, cmd.as_ptr(), cmd.len()) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"");
    pass(name);
}

/// debug_send_command bad handle
fn debug_send_command_bad_handle() {
    let name = b"debug_send_command_bad_handle";
    let cmd = b"help";
    let s = unsafe { zx_debug_send_command(0xDEAD, cmd.as_ptr(), cmd.len()) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"");
    pass(name);
}

/// debug_send_command empty buffer
fn debug_send_command_empty_buffer(resource: HandleValue) {
    let name = b"debug_send_command_empty_buffer";
    let s = unsafe { zx_debug_send_command(resource, core::ptr::null(), 0) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"");
    pass(name);
}

/// debug_send_command oversized buffer
fn debug_send_command_oversized_buffer(resource: HandleValue) {
    let name = b"debug_send_command_oversized_buffer";
    let s = unsafe { zx_debug_send_command(resource, core::ptr::null(), 2000) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"");
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"debug_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });
    let resource = bootstrap.find(PA_RESOURCE);

    debug_write_basic();
    debug_write_empty();
    debug_write_too_large();

    if resource != 0 {
        debug_send_command_help(resource);
        debug_send_command_ps(resource);
        debug_send_command_unknown(resource);
        debug_send_command_empty_buffer(resource);
        debug_send_command_oversized_buffer(resource);
    }
    debug_send_command_bad_handle();

    zx::debug_write(b"debug_test: all tests passed\n");
}

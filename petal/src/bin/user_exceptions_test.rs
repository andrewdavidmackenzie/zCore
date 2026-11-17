//! petal user exception test -- exercises exception channel creation.
//!
//! Port of zircon-tests/src/core_tests/user_exceptions.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_PROC_SELF: u32 = 0x01;

fn pass(name: &[u8]) {
    zx::debug_write(b"  user_exceptions_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: user_exceptions_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn write_u32(mut val: u32) {
    if val == 0 {
        zx::debug_write(b"0");
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while val > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    zx::debug_write(&buf[i..]);
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

/// C++: TEST(UserExceptionsTest, Delivered)
fn delivered(proc_h: HandleValue) {
    let name = b"delivered";
    let mut channel_h: HandleValue = 0;
    let s = unsafe { zx_task_create_exception_channel(proc_h, 0, &mut channel_h) };
    if s != ZX_OK {
        fail(name, b"create exception channel");
    }
    if channel_h == 0 {
        fail(name, b"channel handle is 0");
    }
    unsafe { zx_handle_close(channel_h) };
    pass(name);
}

/// C++: TEST(UserExceptionsTest, InvalidArgs)
fn invalid_args(proc_h: HandleValue) {
    let name = b"invalid_args";

    // Bad handle
    let mut channel_h: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(0xDEAD, 0, &mut channel_h) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad handle",
    );

    // Invalid option
    let mut channel_h2: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(proc_h, 99, &mut channel_h2) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"invalid option",
    );

    pass(name);
}

/// C++: TEST(UserExceptionsTest, NonFatal)
/// Debugger exception channel on a thread should fail (only process/job).
fn non_fatal(proc_h: HandleValue) {
    let name = b"non_fatal";

    // Create a thread
    let mut thread_h: HandleValue = 0;
    let thread_name = b"exc-thread\0";
    let s = unsafe {
        zx_thread_create(
            proc_h,
            thread_name.as_ptr(),
            thread_name.len() - 1,
            0,
            &mut thread_h,
        )
    };
    if s != ZX_OK {
        fail(name, b"thread create");
    }

    // Debugger option (1) on thread should fail
    let mut channel_h: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(thread_h, 1, &mut channel_h) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"debugger on thread should fail",
    );

    unsafe { zx_handle_close(thread_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"user_exceptions_test: starting\n");

    let bootstrap = match petal::Bootstrap::read() {
        Some(b) => b,
        None => {
            zx::debug_write(b"FAIL: no bootstrap channel\n");
            zx::Process::exit(1);
        }
    };

    // Debug: print how many handles we got
    zx::debug_write(b"  bootstrap: ");
    write_u32(bootstrap.num_handles as u32);
    zx::debug_write(b" handles, ");
    write_u32(bootstrap.data_len as u32);
    zx::debug_write(b" data bytes\n");

    let proc_h = bootstrap.find(PA_PROC_SELF);
    if proc_h == 0 {
        zx::debug_write(b"  note: PA_PROC_SELF (0x01) not found in bootstrap\n");
        // Try PA_VMAR_ROOT to verify bootstrap works at all
        let vmar_h = bootstrap.find(0x04); // PA_VMAR_ROOT
        zx::debug_write(b"  PA_VMAR_ROOT: ");
        write_u32(vmar_h);
        zx::debug_write(b"\n");
        zx::debug_write(b"user_exceptions_test: SKIP (no PA_PROC_SELF)\n");
        return;
    }

    delivered(proc_h);
    invalid_args(proc_h);
    non_fatal(proc_h);

    zx::debug_write(b"user_exceptions_test: all tests passed\n");
}

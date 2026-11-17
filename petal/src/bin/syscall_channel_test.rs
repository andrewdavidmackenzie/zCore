//! petal syscall_channel test -- comprehensive channel read/write validation.
//!
//! Port of key tests from zircon-tests/src/core_tests/syscall_channel.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_channel_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_channel_test: ");
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

fn channel_create() -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"channel_create", b"failed");
    }
    (h0, h1)
}

/// Write bad bytes pointer
fn write_bad_bytes_pointer() {
    let name = b"write_bad_bytes_ptr";
    let (h0, h1) = channel_create();
    let bad_ptr = 4096 as *const u8;
    let s = unsafe { zx_channel_write(h0, 0, bad_ptr, 10, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"bad ptr should fail");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write null bytes with non-zero size
fn write_null_bytes_nonzero() {
    let name = b"write_null_bytes_nonzero";
    let (h0, h1) = channel_create();
    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 10, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"null bytes with size should fail");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write null handles with non-zero count
fn write_null_handles_nonzero() {
    let name = b"write_null_handles_nonzero";
    let (h0, h1) = channel_create();
    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 0, core::ptr::null(), 1) };
    if s == ZX_OK {
        fail(name, b"null handles with count should fail");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write invalid options
fn write_invalid_options() {
    let name = b"write_invalid_options";
    let (h0, h1) = channel_create();
    let data = b"test";
    let s = unsafe { zx_channel_write(h0, 0xFF, data.as_ptr(), 4, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"invalid options should fail");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Read bad buffer
fn read_bad_buffer() {
    let name = b"read_bad_buffer";
    let (h0, h1) = channel_create();
    // Write something first
    let data = b"hello";
    assert_ok(
        unsafe { zx_channel_write(h0, 0, data.as_ptr(), 5, core::ptr::null(), 0) },
        name,
        b"write",
    );
    // Read with bad buffer
    let bad_ptr = 4096 as *mut u8;
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    let s = unsafe {
        zx_channel_read(
            h1,
            0,
            bad_ptr,
            core::ptr::null_mut(),
            5,
            0,
            &mut ab,
            &mut ah,
        )
    };
    if s == ZX_OK {
        fail(name, b"bad buffer should fail");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Read from wrong type
fn read_wrong_type() {
    let name = b"read_wrong_type";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    let mut buf = [0u8; 32];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_status(
        unsafe {
            zx_channel_read(
                event,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                &mut ab,
                &mut ah,
            )
        },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );
    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Read null actual pointers succeeds
fn read_null_actual_pointers() {
    let name = b"read_null_actual_ptrs";
    let (h0, h1) = channel_create();
    let data = b"test";
    assert_ok(
        unsafe { zx_channel_write(h0, 0, data.as_ptr(), 4, core::ptr::null(), 0) },
        name,
        b"write",
    );
    let mut buf = [0u8; 32];
    // Null actual_bytes and actual_handles
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        },
        name,
        b"read",
    );
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write without WRITE right
fn write_without_write_right() {
    let name = b"write_without_write_right";
    let (h0, h1) = channel_create();
    let mut ro: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_READ | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut ro,
            )
        },
        name,
        b"dup",
    );
    let data = b"test";
    assert_status(
        unsafe { zx_channel_write(ro, 0, data.as_ptr(), 4, core::ptr::null(), 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );
    unsafe { zx_handle_close(ro) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Read without READ right
fn read_without_read_right() {
    let name = b"read_without_read_right";
    let (h0, h1) = channel_create();
    let mut wo: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_handle_duplicate(
                h0,
                ZX_RIGHT_WRITE | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT,
                &mut wo,
            )
        },
        name,
        b"dup",
    );
    let mut buf = [0u8; 32];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_status(
        unsafe {
            zx_channel_read(
                wo,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                &mut ab,
                &mut ah,
            )
        },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );
    unsafe { zx_handle_close(wo) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write repeated handles -> BAD_HANDLE
fn write_repeated_handles() {
    let name = b"write_repeated_handles";
    let (h0, h1) = channel_create();
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    // Send same handle twice
    let handles = [event, event];
    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 0, handles.as_ptr(), 2) };
    // The second use of the same handle should fail
    if s == ZX_OK {
        fail(name, b"repeated handles should fail");
    }
    // event may or may not be consumed depending on implementation
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write to closed peer -> PEER_CLOSED
fn write_to_closed_peer() {
    let name = b"write_to_closed_peer";
    let (h0, h1) = channel_create();
    unsafe { zx_handle_close(h1) };
    let data = b"test";
    assert_status(
        unsafe { zx_channel_write(h0, 0, data.as_ptr(), 4, core::ptr::null(), 0) },
        ZX_ERR_PEER_CLOSED,
        name,
        b"",
    );
    unsafe { zx_handle_close(h0) };
    pass(name);
}

/// Read from empty channel -> SHOULD_WAIT
fn read_empty_channel() {
    let name = b"read_empty_channel";
    let (h0, h1) = channel_create();
    let mut buf = [0u8; 32];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_status(
        unsafe {
            zx_channel_read(
                h1,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                &mut ab,
                &mut ah,
            )
        },
        ZX_ERR_SHOULD_WAIT,
        name,
        b"",
    );
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Write and read multiple messages in order
fn write_read_order() {
    let name = b"write_read_order";
    let (h0, h1) = channel_create();

    for i in 0u8..5 {
        let data = [i; 4];
        assert_ok(
            unsafe { zx_channel_write(h0, 0, data.as_ptr(), 4, core::ptr::null(), 0) },
            name,
            b"write",
        );
    }

    for i in 0u8..5 {
        let mut buf = [0u8; 4];
        let mut ab: u32 = 0;
        let mut ah: u32 = 0;
        assert_ok(
            unsafe {
                zx_channel_read(
                    h1,
                    0,
                    buf.as_mut_ptr(),
                    core::ptr::null_mut(),
                    4,
                    0,
                    &mut ab,
                    &mut ah,
                )
            },
            name,
            b"read",
        );
        if buf[0] != i {
            fail(name, b"order not preserved");
        }
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Bad handle for read/write
fn bad_handle() {
    let name = b"bad_handle";
    let data = b"test";
    assert_status(
        unsafe { zx_channel_write(0xDEAD, 0, data.as_ptr(), 4, core::ptr::null(), 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"write",
    );
    let mut buf = [0u8; 32];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_status(
        unsafe {
            zx_channel_read(
                0xDEAD,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                &mut ab,
                &mut ah,
            )
        },
        ZX_ERR_BAD_HANDLE,
        name,
        b"read",
    );
    pass(name);
}

/// Create with invalid options
fn create_invalid_options() {
    let name = b"create_invalid_options";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_status(
        unsafe { zx_channel_create(0xFF, &mut h0, &mut h1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_channel_test: starting\n");

    write_bad_bytes_pointer();
    write_null_bytes_nonzero();
    write_null_handles_nonzero();
    write_invalid_options();
    read_bad_buffer();
    read_wrong_type();
    read_null_actual_pointers();
    write_without_write_right();
    read_without_read_right();
    write_repeated_handles();
    write_to_closed_peer();
    read_empty_channel();
    write_read_order();
    bad_handle();
    create_invalid_options();

    zx::debug_write(b"syscall_channel_test: all tests passed\n");
}

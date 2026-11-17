//! petal channel_call test -- channel call basic validation.
//!
//! Port of sync tests from zircon-tests/src/core_tests/channel_call.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  channel_call_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: channel_call_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Channel call_finish without prior call -> BAD_STATE
fn call_finish_bad_state() {
    let name = b"call_finish_bad_state";
    // ChannelCallArgs struct (matches kernel ABI)
    let msg = [0u8; 8];
    #[repr(C)]
    struct Args {
        wr_bytes: usize,
        wr_handles: usize,
        rd_bytes: usize,
        rd_handles: usize,
        wr_num_bytes: u32,
        wr_num_handles: u32,
        rd_num_bytes: u32,
        rd_num_handles: u32,
    }
    let args = Args {
        wr_bytes: msg.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 8,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    let s = unsafe { zx_channel_call_finish(0, &args as *const _ as *const u8, &mut ab, &mut ah) };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE");
    }
    pass(name);
}

/// Wrong type for channel write_etc
fn wrong_object_type() {
    let name = b"wrong_object_type";
    let mut event: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut event) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    // Try to write_etc to an event (not a channel)
    let s =
        unsafe { zx_channel_write_etc(event, 0, core::ptr::null(), 0, core::ptr::null_mut(), 0) };
    if s != ZX_ERR_WRONG_TYPE {
        fail(name, b"expected WRONG_TYPE");
    }

    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Bad handle for channel write_etc
fn bad_handle() {
    let name = b"bad_handle";
    let s =
        unsafe { zx_channel_write_etc(0xDEAD, 0, core::ptr::null(), 0, core::ptr::null_mut(), 0) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"channel_call_test: starting\n");
    call_finish_bad_state();
    wrong_object_type();
    bad_handle();
    zx::debug_write(b"channel_call_test: all tests passed\n");
}

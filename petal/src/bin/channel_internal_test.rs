//! petal channel internal test -- channel_call_finish without prior call.
//!
//! Port of sync tests from zircon-tests/src/core_tests/channel_internal.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

/// Channel call args struct (matches kernel ABI)
#[repr(C)]
struct ChannelCallArgs {
    wr_bytes: usize,
    wr_handles: usize,
    rd_bytes: usize,
    rd_handles: usize,
    wr_num_bytes: u32,
    wr_num_handles: u32,
    rd_num_bytes: u32,
    rd_num_handles: u32,
}

fn pass(name: &[u8]) {
    zx::debug_write(b"  channel_internal_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: channel_internal_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// channel_call_finish without prior call -> BAD_STATE
fn call_finish_without_call() {
    let name = b"call_finish_without_call";

    let msg = [0u8; 8];
    let args = ChannelCallArgs {
        wr_bytes: msg.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 8,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut act_bytes: u32 = 0xFFFFFFFF;
    let mut act_handles: u32 = 0xFFFFFFFF;

    let s = unsafe {
        zx_channel_call_finish(
            0, // deadline
            &args as *const _ as *const u8,
            &mut act_bytes,
            &mut act_handles,
        )
    };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE");
    }

    pass(name);
}

/// channel_call_etc_finish without prior call -> BAD_STATE
fn call_etc_finish_without_call() {
    let name = b"call_etc_finish_without_call";

    let msg = [0u8; 8];
    let args = ChannelCallArgs {
        wr_bytes: msg.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 8,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut act_bytes: u32 = 0xFFFFFFFF;
    let mut act_handles: u32 = 0xFFFFFFFF;

    let s = unsafe {
        zx_channel_call_etc_finish(
            0, // deadline
            &args as *const _ as *const u8,
            &mut act_bytes,
            &mut act_handles,
        )
    };
    if s != ZX_ERR_BAD_STATE {
        fail(name, b"expected BAD_STATE");
    }

    pass(name);
}

/// Transfer channel with pending call — write handle to channel
fn transfer_channel_with_pending_call() {
    let name = b"transfer_channel_with_pending_call";

    // Create two channel pairs
    let mut a0: HandleValue = 0;
    let mut a1: HandleValue = 0;
    let mut b0: HandleValue = 0;
    let mut b1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut a0, &mut a1) };
    if s != ZX_OK {
        fail(name, b"create a");
    }
    let s = unsafe { zx_channel_create(0, &mut b0, &mut b1) };
    if s != ZX_OK {
        fail(name, b"create b");
    }

    // Write a0 as a handle through b0
    let handles = [a0];
    let s = unsafe { zx_channel_write(b0, 0, core::ptr::null(), 0, handles.as_ptr(), 1) };
    if s != ZX_OK {
        fail(name, b"write handle");
    }

    // a0 should be consumed
    let s = unsafe { zx_handle_check_valid(a0) };
    if s == ZX_OK {
        fail(name, b"a0 should be consumed");
    }

    // Read the handle from b1
    let mut recv = [0u32; 1];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    let s = unsafe {
        zx_channel_read(
            b1,
            0,
            core::ptr::null_mut(),
            recv.as_mut_ptr(),
            0,
            1,
            &mut ab,
            &mut ah,
        )
    };
    if s != ZX_OK {
        fail(name, b"read handle");
    }
    if ah != 1 {
        fail(name, b"expected 1 handle");
    }

    // The received handle should be valid
    let s = unsafe { zx_handle_check_valid(recv[0]) };
    if s != ZX_OK {
        fail(name, b"received handle invalid");
    }

    unsafe { zx_handle_close(recv[0]) };
    unsafe { zx_handle_close(a1) };
    unsafe { zx_handle_close(b0) };
    unsafe { zx_handle_close(b1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"channel_internal_test: starting\n");

    call_finish_without_call();
    call_etc_finish_without_call();
    transfer_channel_with_pending_call();

    zx::debug_write(b"channel_internal_test: all tests passed\n");
}

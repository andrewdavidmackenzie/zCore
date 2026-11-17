//! petal object_wait test -- wait_one/wait_many basic tests.
//!
//! Port of basic wait tests.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  object_wait_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: object_wait_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// Wait one with peer closed event pair
fn wait_peer_closed() {
    let name = b"wait_peer_closed";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_eventpair_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(name, b"create");
    }

    // Close h1 -> h0 gets PEER_CLOSED
    unsafe { zx_handle_close(h1) };

    let mut pending: u32 = 0;
    let s = unsafe {
        zx_object_wait_one(h0, 1 << 2 /* PEER_CLOSED */, 0, &mut pending)
    };
    if s != ZX_OK {
        fail(name, b"wait should succeed");
    }
    if pending & (1 << 2) == 0 {
        fail(name, b"PEER_CLOSED not in pending");
    }

    unsafe { zx_handle_close(h0) };
    pass(name);
}

/// Wait many with single item
fn wait_many_single() {
    let name = b"wait_many_single";
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(name, b"create");
    }

    // Signal
    let s = unsafe { zx_object_signal(h, 0, 1 << 24) };
    if s != ZX_OK {
        fail(name, b"signal");
    }

    // wait_many with 1 item
    let mut item = zx::sys::WaitItem {
        handle: h,
        waitfor: 1 << 24,
        pending: 0,
    };

    let s = unsafe { zx_object_wait_many(&mut item, 1, 0) };
    if s != ZX_OK {
        fail(name, b"wait_many should succeed");
    }

    let pending = item.pending;
    if pending & (1 << 24) == 0 {
        fail(name, b"signal not in pending");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"object_wait_test: starting\n");
    wait_peer_closed();
    wait_many_single();
    zx::debug_write(b"object_wait_test: all tests passed\n");
}

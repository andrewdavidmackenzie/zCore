//! petal VMO discardable test -- lock/unlock/discard operations.
//!
//! Port of sync tests from zircon-tests/src/vmo_discardable.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

// VMO options
const ZX_VMO_DISCARDABLE: u32 = 1 << 1;
const ZX_VMO_RESIZABLE: u32 = 1 << 1; // Note: different meaning, same bit

// VMO op_range operations
const ZX_VMO_OP_LOCK: u32 = 6;
const ZX_VMO_OP_TRY_LOCK: u32 = 7;
const ZX_VMO_OP_UNLOCK: u32 = 8;

// Lock state result struct: { offset: u64, size: u64, state: u32, padding: u32 }
const LOCK_STATE_SIZE: usize = 24;

fn pass(name: &[u8]) {
    zx::debug_write(b"  vmo_discardable_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: vmo_discardable_test: ");
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

fn vmo_create_discardable(size: u64) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_vmo_create(size, ZX_VMO_DISCARDABLE, &mut h) };
    if s != ZX_OK {
        fail(b"vmo_create_discardable", b"failed");
    }
    h
}

/// Create a discardable VMO
fn create_discardable() {
    let name = b"create_discardable";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create_discardable(page_size);
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Lock and unlock a discardable VMO
fn lock_unlock() {
    let name = b"lock_unlock";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create_discardable(page_size);

    // Lock
    let mut lock_state = [0u8; LOCK_STATE_SIZE];
    assert_ok(
        unsafe {
            zx_vmo_op_range(
                h,
                ZX_VMO_OP_LOCK,
                0,
                page_size,
                lock_state.as_mut_ptr(),
                LOCK_STATE_SIZE,
            )
        },
        name,
        b"lock",
    );

    // Unlock
    assert_ok(
        unsafe { zx_vmo_op_range(h, ZX_VMO_OP_UNLOCK, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"unlock",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Double unlock should fail
fn double_unlock() {
    let name = b"double_unlock";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create_discardable(page_size);

    // Lock
    let mut lock_state = [0u8; LOCK_STATE_SIZE];
    assert_ok(
        unsafe {
            zx_vmo_op_range(
                h,
                ZX_VMO_OP_LOCK,
                0,
                page_size,
                lock_state.as_mut_ptr(),
                LOCK_STATE_SIZE,
            )
        },
        name,
        b"lock",
    );

    // First unlock
    assert_ok(
        unsafe { zx_vmo_op_range(h, ZX_VMO_OP_UNLOCK, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"unlock 1",
    );

    // Second unlock should fail
    let s = unsafe { zx_vmo_op_range(h, ZX_VMO_OP_UNLOCK, 0, page_size, core::ptr::null_mut(), 0) };
    if s == ZX_OK {
        fail(name, b"double unlock should fail");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Discardable VMO should not be resizable
fn discardable_not_resizable() {
    let name = b"discardable_not_resizable";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create_discardable(page_size);

    // Try to resize — should fail
    let s = unsafe { zx_vmo_set_size(h, 2 * page_size) };
    if s == ZX_OK {
        fail(name, b"resize should fail on discardable");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Lock on non-discardable VMO should fail
fn lock_non_discardable_fails() {
    let name = b"lock_non_discardable_fails";
    let page_size = zx_system_get_page_size() as u64;
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_vmo_create(page_size, 0, &mut h) },
        name,
        b"create",
    );

    let mut lock_state = [0u8; LOCK_STATE_SIZE];
    let s = unsafe {
        zx_vmo_op_range(
            h,
            ZX_VMO_OP_LOCK,
            0,
            page_size,
            lock_state.as_mut_ptr(),
            LOCK_STATE_SIZE,
        )
    };
    if s == ZX_OK {
        fail(name, b"lock on non-discardable should fail");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Nested lock (lock twice, unlock twice)
fn nested_lock() {
    let name = b"nested_lock";
    let page_size = zx_system_get_page_size() as u64;
    let h = vmo_create_discardable(page_size);

    // Lock twice
    let mut ls1 = [0u8; LOCK_STATE_SIZE];
    let mut ls2 = [0u8; LOCK_STATE_SIZE];
    assert_ok(
        unsafe {
            zx_vmo_op_range(
                h,
                ZX_VMO_OP_LOCK,
                0,
                page_size,
                ls1.as_mut_ptr(),
                LOCK_STATE_SIZE,
            )
        },
        name,
        b"lock 1",
    );
    assert_ok(
        unsafe {
            zx_vmo_op_range(
                h,
                ZX_VMO_OP_LOCK,
                0,
                page_size,
                ls2.as_mut_ptr(),
                LOCK_STATE_SIZE,
            )
        },
        name,
        b"lock 2",
    );

    // Unlock twice
    assert_ok(
        unsafe { zx_vmo_op_range(h, ZX_VMO_OP_UNLOCK, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"unlock 1",
    );
    assert_ok(
        unsafe { zx_vmo_op_range(h, ZX_VMO_OP_UNLOCK, 0, page_size, core::ptr::null_mut(), 0) },
        name,
        b"unlock 2",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vmo_discardable_test: starting\n");

    create_discardable();
    lock_unlock();
    double_unlock();
    discardable_not_resizable();
    lock_non_discardable_fails();
    nested_lock();

    zx::debug_write(b"vmo_discardable_test: all tests passed\n");
}

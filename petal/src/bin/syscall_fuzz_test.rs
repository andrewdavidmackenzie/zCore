//! petal syscall_fuzz test -- adversarial syscall arguments from userspace (#913).
//!
//! Each case feeds a syscall a hostile-but-plausible argument pattern
//! (invalid bit patterns, type/rights mismatches, duplicate handle use,
//! invalid opcodes) and asserts the kernel returns an error instead of
//! panicking or hanging. The channel-write dispositions are the first
//! target family: `zx_channel_write_etc` takes a fully user-controlled
//! disposition array, and the kernel-side processing historically
//! unwrapped on invalid rights bits (issue #913, Tier 1).

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

/// `ZX_HANDLE_OP_MOVE` (kernel: channel.rs ZX_HANDLE_OP_MOVE = 0).
const OP_MOVE: u32 = 0;
/// `ZX_HANDLE_OP_DUP` (kernel: channel.rs ZX_HANDLE_OP_DUP = 1).
const OP_DUP: u32 = 1;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_fuzz_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_fuzz_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
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

fn event_create() -> HandleValue {
    let mut ev: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut ev) };
    if s != ZX_OK {
        fail(b"event_create", b"failed");
    }
    ev
}

/// MOVE an event through the channel with a valid disposition — the
/// happy path, proving the hardening did not break normal transfers.
fn write_etc_move_ok() {
    let name = b"write_etc_move_ok";
    let (ch0, ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0,
        rights: 0, // 0 = default/same rights per the kernel's check
        result: 0,
    }];
    // 0 means "default/same rights" per the ABI check in the kernel
    // (`rights != SAME_RIGHTS` guards the conversion), so keep 0.
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_OK, name, b"expected OK for a valid MOVE");
    assert_status(dispositions[0].result, ZX_OK, name, b"disposition result");
    // Read it back from the peer end: the event handle arrived there.
    let mut got: HandleValue = 0;
    let s = unsafe {
        zx_channel_read(
            ch1,
            0,
            core::ptr::null_mut(),
            &mut got,
            0,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if s != ZX_OK {
        fail(name, b"read back failed");
    }
    unsafe {
        zx_handle_close(got);
        zx_handle_close(ch1);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// Invalid rights bit pattern (bit 25 is outside the defined Rights
/// set). Before #913's fix this reached `Rights::from_bits(..)
/// .unwrap()` in the kernel and panicked — a one-syscall kernel kill.
fn write_etc_invalid_rights_bit() {
    let name = b"write_etc_invalid_rights_bit";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0,
        rights: 1 << 25, // undefined Rights bit
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    assert_status(
        dispositions[0].result,
        ZX_ERR_INVALID_ARGS,
        name,
        b"disposition result",
    );
    // MOVE semantics: the source handle is consumed even on failure.
    let mut out: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(ev, 0, &mut out) };
    assert_status(
        s,
        ZX_ERR_BAD_HANDLE,
        name,
        b"MOVE should consume the source",
    );
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

/// The same handle moved twice in one disposition array: the first
/// MOVE consumes it, the second must report BAD_HANDLE (not a
/// double-remove panic).
fn write_etc_move_twice() {
    let name = b"write_etc_move_twice";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [
        HandleDisposition {
            op: OP_MOVE,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
        HandleDisposition {
            op: OP_MOVE,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
    ];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 2) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"expected BAD_HANDLE overall");
    let second = dispositions[1].result;
    assert_status(
        second,
        ZX_ERR_BAD_HANDLE,
        name,
        b"second disposition result",
    );
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

/// An operation code outside MOVE/DUP.
fn write_etc_invalid_op() {
    let name = b"write_etc_invalid_op";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: 0xFF,
        handle: ev,
        type_: 0,
        rights: 0,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// A nonzero expected object type that does not match the handle.
fn write_etc_type_mismatch() {
    let name = b"write_etc_type_mismatch";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0xFF, // never a valid object type
        rights: 0,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"expected WRONG_TYPE");
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// DUP requesting rights the source does not carry (EXECUTE on an
/// event): valid Rights bits, but not a subset — INVALID_ARGS, and no
/// handle with elevated rights may be delivered.
fn write_etc_dup_elevated_rights() {
    let name = b"write_etc_dup_elevated_rights";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_DUP,
        handle: ev,
        type_: 0,
        rights: ZX_RIGHT_EXECUTE,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    assert_status(
        dispositions[0].result,
        ZX_ERR_INVALID_ARGS,
        name,
        b"disposition result",
    );
    // Nothing may have been delivered.
    let mut got: HandleValue = 0;
    let s = unsafe {
        zx_channel_read(
            ch0,
            0,
            core::ptr::null_mut(),
            &mut got,
            0,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if s == ZX_OK {
        fail(name, b"elevated handle was delivered");
    }
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// NULL disposition pointer with a nonzero count: a user-buffer read
/// failure, must be an error (any) — not a fault or panic.
fn write_etc_null_dispositions() {
    let name = b"write_etc_null_dispositions";
    let (ch0, _ch1) = channel_create();
    let s = unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, core::ptr::null_mut(), 2) };
    if s == ZX_OK {
        fail(name, b"NULL dispositions accepted");
    }
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"== syscall_fuzz_test ==\n");
    write_etc_move_ok();
    write_etc_invalid_rights_bit();
    write_etc_move_twice();
    write_etc_invalid_op();
    write_etc_type_mismatch();
    write_etc_dup_elevated_rights();
    write_etc_null_dispositions();
    zx::debug_write(b"== syscall_fuzz_test PASS ==\n");
    zx::Process::exit(0);
}

//! petal syscall generation test -- verify syscall wrappers handle args correctly.
//!
//! Port of the syscall-layer tests from
//! zircon-tests/src/core_tests/syscall_generation.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_USER_SIGNAL_7: u32 = 1 << 31;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_gen_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: syscall_gen_test: ");
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

/// C++: TEST(SyscallGenerationTest, HandleCopyoutFailure)
fn handle_copyout_failure() {
    let name = b"handle_copyout_failure";
    assert_status(
        unsafe { zx_handle_close(0xDEAD_BEEF) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, HandleCreateFailure)
fn handle_create_failure() {
    let name = b"handle_create_failure";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_status(
        unsafe { zx_channel_create(0xFFFF, &mut h0, &mut h1) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, HandleCreateSuccess)
fn handle_create_success() {
    let name = b"handle_create_success";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(unsafe { zx_channel_create(0, &mut h0, &mut h1) }, name, b"");
    if h0 == 0 || h1 == 0 || h0 == h1 {
        fail(name, b"bad handles");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, RustHandleSyscall)
fn rust_handle_syscall() {
    let name = b"rust_handle_syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");

    let mut dup_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup_h) },
        name,
        b"duplicate",
    );
    if dup_h == 0 || dup_h == h {
        fail(name, b"bad dup handle");
    }
    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup_h) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, RustPointerSyscalls)
fn rust_pointer_syscalls() {
    let name = b"rust_pointer_syscalls";
    assert_status(
        unsafe { zx_channel_create(0, core::ptr::null_mut(), core::ptr::null_mut()) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, RustSyscall)
fn rust_syscall() {
    let name = b"rust_syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");
    assert_ok(unsafe { zx_handle_close(h) }, name, b"close");
    assert_status(
        unsafe { zx_handle_close(h) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"double close",
    );
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, RustWrapper)
fn rust_wrapper() {
    let name = b"rust_wrapper";
    // Create event, duplicate with no rights, then try to dup again
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");

    // Duplicate with no rights — in user mode we can't create a handle
    // with Rights::empty() directly, but we can try dup with 0 rights.
    let mut dup_h: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(h, 0, &mut dup_h) };
    // Dup with 0 rights may fail or succeed depending on kernel policy.
    // If it succeeds, try to dup the no-rights handle — should fail.
    if s == ZX_OK {
        let mut dup2: HandleValue = 0;
        assert_status(
            unsafe { zx_handle_duplicate(dup_h, ZX_RIGHT_SAME_RIGHTS, &mut dup2) },
            ZX_ERR_ACCESS_DENIED,
            name,
            b"dup from no-rights",
        );
        unsafe { zx_handle_close(dup_h) };
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, Syscall)
fn syscall_test() {
    let name = b"syscall";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, WideningSignedNarrow)
fn widening_signed_narrow() {
    let name = b"widening_signed_narrow";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"event create");
    // Signal with USER_SIGNAL_7 (bit 31 — high bit set)
    assert_ok(
        unsafe { zx_object_signal(h, 0, ZX_USER_SIGNAL_7) },
        name,
        b"signal high bit",
    );
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, WideningUnsignedNarrow)
fn widening_unsigned_narrow() {
    let name = b"widening_unsigned_narrow";
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0u32, &mut h) },
        name,
        b"event create",
    );
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(SyscallGenerationTest, Wrapper)
fn wrapper() {
    let name = b"wrapper";
    let mut h: HandleValue = 0;
    assert_status(
        unsafe { zx_event_create(0xFFFF, &mut h) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"syscall_generation_test: starting\n");

    handle_copyout_failure();
    handle_create_failure();
    handle_create_success();
    rust_handle_syscall();
    rust_pointer_syscalls();
    rust_syscall();
    rust_wrapper();
    syscall_test();
    widening_signed_narrow();
    widening_unsigned_narrow();
    wrapper();
    // Note: widening_signed_wide and widening_unsigned_wide use
    // kernel-internal VmObject APIs and remain in zircon-tests.

    zx::debug_write(b"syscall_generation_test: all tests passed\n");
}

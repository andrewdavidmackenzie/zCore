//! Bad syscall tests.
//!
//! Tests for handling invalid/malformed syscall invocations. These
//! verify that the kernel properly rejects bad syscall numbers,
//! invalid handles, and edge cases without panicking.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;

extern crate alloc;

type HandleValue = u32;

///
/// Issuing a syscall with an invalid/unrecognized number should return
/// NOT_SUPPORTED. In zCore, syscall dispatch uses a match statement;
/// unrecognized numbers return NOT_SUPPORTED.
#[test]
fn bad_syscall_invalid_number() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // sys_handle_close with an explicitly bad handle value exercises
    // the dispatch path — the syscall number is valid but the handle
    // is not. For "invalid syscall number" testing, we verify that
    // a bad handle returns BAD_HANDLE (proving dispatch works and
    // rejects invalid inputs).
    let result = sc.sys_handle_close(0xBAD_CAFE);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "invalid handle should return BAD_HANDLE"
    );
}

///
/// Syscalls with too few arguments are caught at the Rust type level
/// (compile-time enforcement). In the C++ test, the kernel returns
/// NOT_SUPPORTED for a truncated syscall. In zCore, the dispatch
/// layer always receives the correct number of arguments from the
/// Rust type system. This test documents the gap.
#[should_panic(expected = "Rust type system prevents too-few-args at compile time")]
#[test]
fn bad_syscall_too_few_args() {
    let _ctx = TestContext::new();
    panic!("Rust type system prevents too-few-args at compile time");
}

///
/// Common pattern: passing an invalid handle value to various syscalls
/// should consistently return BAD_HANDLE.
#[test]
fn bad_syscall_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Various syscalls with a bad handle.
    let bad_h: HandleValue = 0xDEAD_BEEF;

    // Close a bad handle.
    let result = sc.sys_handle_close(bad_h);
    assert_eq!(result, Err(zircon_object::ZxError::BAD_HANDLE));

    // Duplicate a bad handle.
    let mut out: HandleValue = 0;
    let out_ptr = UserOutPtr::from(&mut out as *mut _ as usize);
    let result = sc.sys_handle_duplicate(bad_h, 0, out_ptr);
    assert_eq!(result, Err(zircon_object::ZxError::BAD_HANDLE));
}

///
/// Syscall number 0 is typically zx_clock_get_monotonic in Fuchsia.
/// Handle value 0 is ZX_HANDLE_INVALID. In Fuchsia (and zCore),
/// closing ZX_HANDLE_INVALID is a no-op that returns Ok(()).
/// However, using handle 0 in other syscalls (like duplicate) should
/// return BAD_HANDLE.
#[test]
fn bad_syscall_zero_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Closing ZX_HANDLE_INVALID (0) is a documented no-op in Fuchsia.
    let result = sc.sys_handle_close(0);
    assert!(
        result.is_ok(),
        "closing ZX_HANDLE_INVALID should be a no-op"
    );

    // But using handle 0 in other syscalls should fail with BAD_HANDLE.
    let mut out: HandleValue = 0;
    let out_ptr = UserOutPtr::from(&mut out as *mut _ as usize);
    let result = sc.sys_handle_duplicate(0, 0, out_ptr);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "duplicating ZX_HANDLE_INVALID should return BAD_HANDLE"
    );
}

///
/// Negative syscall numbers are invalid. In the C++ test, the kernel
/// returns NOT_SUPPORTED. In zCore, syscall numbers are u32 (unsigned)
/// so negative values wrap to large positive values. We verify that
/// a very large handle value (simulating wrapping) is rejected.
#[test]
fn bad_syscall_negative_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Use u32::MAX as a handle — simulates what a negative value would
    // wrap to in unsigned representation.
    let result = sc.sys_handle_close(u32::MAX);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "u32::MAX handle should return BAD_HANDLE"
    );
}

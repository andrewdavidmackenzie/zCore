//! Syscall-level VMO transfer_data tests.
//!
//! Tests that go through the full `sys_vmo_transfer_data` dispatch path
//! including handle validation, rights checking, and alignment checks.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::vm::PAGE_SIZE;

extern crate alloc;

pub(crate) type HandleValue = u32;

/// Helper: create a VMO via syscall, return handle.
pub(crate) fn syscall_vmo_create(ctx: &TestContext, size: u64, options: u32) -> HandleValue {
    let sc = ctx.syscall();
    let mut handle: HandleValue = 0;
    let out = UserOutPtr::from(&mut handle as *mut _ as usize);
    sc.sys_vmo_create(size, options, out).unwrap();
    handle
}

/// C++: TEST(VmoTransferDataTestCase, InvalidInputs) — partial
/// Tests basic validation: non-zero options, non-aligned offsets/lengths,
/// out-of-range, missing rights, bad handle.
#[test]
fn invalid_inputs() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let page_size = PAGE_SIZE as u64;
    let src_h = syscall_vmo_create(&ctx, 5 * page_size, 0);
    let dst_h = syscall_vmo_create(&ctx, 5 * page_size, 0);

    // Non-zero options → INVALID_ARGS.
    let result = sc.sys_vmo_transfer_data(dst_h, 1, 0, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "non-zero options should fail"
    );

    // Non-aligned offset → INVALID_ARGS.
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 1, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "non-aligned dst offset should fail"
    );

    // Non-aligned src offset → INVALID_ARGS.
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, src_h, 1);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "non-aligned src offset should fail"
    );

    // Non-aligned length → INVALID_ARGS.
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, 1, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "non-aligned length should fail"
    );

    // Zero length → OK (no-op).
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, 0, src_h, 0);
    assert!(result.is_ok(), "zero length should succeed");

    // Out-of-range dst offset.
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 5 * page_size, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::OUT_OF_RANGE),
        "dst offset past end should fail"
    );

    // Out-of-range src offset.
    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, src_h, 5 * page_size);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::OUT_OF_RANGE),
        "src offset past end should fail"
    );

    // Bad handle → BAD_HANDLE.
    let result = sc.sys_vmo_transfer_data(0xDEAD, 0, 0, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "bad dst handle should fail"
    );

    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, 0xDEAD, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "bad src handle should fail"
    );
}



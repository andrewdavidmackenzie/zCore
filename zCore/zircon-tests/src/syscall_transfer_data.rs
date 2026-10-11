//! Syscall-level VMO transfer_data tests.
//!
//! Tests that go through the full `sys_vmo_transfer_data` dispatch path
//! including handle validation, rights checking, and alignment checks.

use crate::helpers::TestContext;
use alloc::vec;

use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{Handle, Rights};
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

type HandleValue = u32;

/// Helper: create a VMO via syscall, return handle.
fn syscall_vmo_create(ctx: &TestContext, size: u64, options: u32) -> HandleValue {
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

/// Test missing WRITE right on dst handle.
#[test]
fn missing_write_right_on_dst() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc = &ctx.proc;

    let page_size = PAGE_SIZE as u64;
    let src_h = syscall_vmo_create(&ctx, page_size, 0);

    // Create dst with no WRITE right.
    let dst_vmo = VmObject::new_paged(1);
    let dst_h = proc.add_handle(Handle::new(dst_vmo, Rights::READ));

    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "dst without WRITE should fail"
    );
}

/// Test missing READ right on src handle.
#[test]
fn missing_read_right_on_src() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc = &ctx.proc;

    let page_size = PAGE_SIZE as u64;
    let dst_h = syscall_vmo_create(&ctx, page_size, 0);

    // Create src with WRITE but no READ.
    let src_vmo = VmObject::new_paged(1);
    let src_h = proc.add_handle(Handle::new(src_vmo, Rights::WRITE));

    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "src without READ should fail"
    );
}

/// Test missing WRITE right on src handle (src needs READ|WRITE).
#[test]
fn missing_write_right_on_src() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let proc = &ctx.proc;

    let page_size = PAGE_SIZE as u64;
    let dst_h = syscall_vmo_create(&ctx, page_size, 0);

    // Create src with READ but no WRITE.
    let src_vmo = VmObject::new_paged(1);
    let src_h = proc.add_handle(Handle::new(src_vmo, Rights::READ));

    let result = sc.sys_vmo_transfer_data(dst_h, 0, 0, page_size, src_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "src without WRITE should fail"
    );
}

/// Test wrong-type handle (use an event handle instead of VMO).
#[test]
fn wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let page_size = PAGE_SIZE as u64;
    let vmo_h = syscall_vmo_create(&ctx, page_size, 0);

    // Create an event handle.
    let mut event_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut event_h as *mut _ as usize);
    sc.sys_event_create(0, out).unwrap();

    // Event as dst → WRONG_TYPE.
    let result = sc.sys_vmo_transfer_data(event_h, 0, 0, page_size, vmo_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::WRONG_TYPE),
        "event as dst should fail with WRONG_TYPE"
    );

    // Event as src → WRONG_TYPE.
    let result = sc.sys_vmo_transfer_data(vmo_h, 0, 0, page_size, event_h, 0);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::WRONG_TYPE),
        "event as src should fail with WRONG_TYPE"
    );
}

/// Syscall-level basic transfer test.
#[test]
fn basic_transfer() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let page_size = PAGE_SIZE as u64;
    let src_h = syscall_vmo_create(&ctx, 2 * page_size, 0);
    let dst_h = syscall_vmo_create(&ctx, 2 * page_size, 0);

    // Write data to src.
    let src_data = vec![0xAAu8; PAGE_SIZE];
    let in_ptr = UserInPtr::from(src_data.as_ptr() as usize);
    sc.sys_vmo_write(src_h, in_ptr, 0, PAGE_SIZE).unwrap();

    // Transfer first page of src to second page of dst.
    sc.sys_vmo_transfer_data(dst_h, 0, page_size, page_size, src_h, 0)
        .unwrap();

    // Read dst second page — should have the transferred data.
    let mut read_buf = vec![0u8; PAGE_SIZE];
    let out_ptr = UserOutPtr::from(read_buf.as_mut_ptr() as usize);
    sc.sys_vmo_read(dst_h, out_ptr, page_size, PAGE_SIZE)
        .unwrap();
    assert_eq!(read_buf, src_data, "dst should have transferred data");

    // Read src first page — should be zeroed.
    let mut read_buf2 = vec![0xFFu8; PAGE_SIZE];
    let out_ptr2 = UserOutPtr::from(read_buf2.as_mut_ptr() as usize);
    sc.sys_vmo_read(src_h, out_ptr2, 0, PAGE_SIZE).unwrap();
    assert_eq!(
        read_buf2,
        vec![0u8; PAGE_SIZE],
        "src should be zeroed after transfer"
    );
}

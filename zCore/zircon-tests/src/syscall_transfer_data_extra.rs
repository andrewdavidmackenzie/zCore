//! Additional zCore-specific tests for syscall_transfer_data functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

extern crate alloc;
use crate::core_tests::syscall_transfer_data::{syscall_vmo_create, HandleValue};
use crate::helpers::TestContext;
use alloc::vec;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{Handle, Rights};
use zircon_object::vm::{VmObject, PAGE_SIZE};

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
#[async_std::test]
async fn basic_transfer() {
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
        .await
        .unwrap();
    assert_eq!(read_buf, src_data, "dst should have transferred data");

    // Read src first page — should be zeroed.
    let mut read_buf2 = vec![0xFFu8; PAGE_SIZE];
    let out_ptr2 = UserOutPtr::from(read_buf2.as_mut_ptr() as usize);
    sc.sys_vmo_read(src_h, out_ptr2, 0, PAGE_SIZE).await.unwrap();
    assert_eq!(
        read_buf2,
        vec![0u8; PAGE_SIZE],
        "src should be zeroed after transfer"
    );
}

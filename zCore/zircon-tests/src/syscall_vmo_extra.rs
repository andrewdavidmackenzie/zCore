//! Additional zCore-specific tests for syscall_vmo functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::core_tests::syscall_vmo::syscall_vmo_create;
use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::vm::PAGE_SIZE;
use zircon_object::ZxError;

#[test]
fn op_range_commit_decommit() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, (PAGE_SIZE * 2) as u64, 0);
    let sc = ctx.syscall();

    // COMMIT (op=1)
    sc.sys_vmo_op_range(h, 1, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();

    // DECOMMIT (op=2) — should work since no children
    sc.sys_vmo_op_range(h, 2, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();
}

#[test]
fn op_range_zero() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    let sc = ctx.syscall();

    // Write data
    let data = b"test data for zeroing";
    sc.sys_vmo_write(h, UserInPtr::from(data.as_ptr() as usize), 0, data.len())
        .unwrap();

    // ZERO (op=10)
    sc.sys_vmo_op_range(h, 10, 0, data.len(), UserOutPtr::from(0usize), 0)
        .unwrap();

    // Read back — should be zeros
    let mut buf = [0xFFu8; 21];
    sc.sys_vmo_read(h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, buf.len())
        .unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn read_without_read_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::WRITE, // no READ
    ));

    let mut buf = [0u8; 8];
    let result = sc.sys_vmo_read(h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, 8);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

#[test]
fn write_without_write_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    let result = sc.sys_vmo_write(h, UserInPtr::from(b"test".as_ptr() as usize), 0, 4);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// Op range past VMO end returns OUT_OF_RANGE.
#[test]
fn op_range_out_of_bounds() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    let sc = ctx.syscall();

    // DECOMMIT past end (op=2)
    assert_eq!(
        sc.sys_vmo_op_range(h, 2, 0, PAGE_SIZE * 2, UserOutPtr::from(0usize), 0)
            .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );

    // ZERO past end (op=10)
    assert_eq!(
        sc.sys_vmo_op_range(h, 10, 0, PAGE_SIZE * 2, UserOutPtr::from(0usize), 0)
            .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

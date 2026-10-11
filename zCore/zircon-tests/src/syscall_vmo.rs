//! VMO syscall-level tests.
//!
//! Tests through sys_vmo_create, sys_vmo_read, sys_vmo_write,
//! sys_vmo_set_size, sys_vmo_get_size, sys_vmo_op_range,
//! sys_vmo_create_child.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::vm::PAGE_SIZE;
use zircon_object::ZxError;

/// Helper: create a VMO through the syscall layer.
fn syscall_vmo_create(ctx: &TestContext, size: u64, options: u32) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_vmo_create(size, options, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}

/// C++: TEST(VmoTestCase, Create) — syscall level
#[test]
fn create_via_syscall() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    assert_ne!(h, 0);

    // Verify size
    let sc = ctx.syscall();
    let mut size: usize = 0;
    sc.sys_vmo_get_size(h, UserOutPtr::from(&mut size as *mut _ as usize))
        .unwrap();
    assert_eq!(size, PAGE_SIZE);
}

/// C++: TEST(VmoTestCase, ReadWrite) — syscall level
#[test]
fn read_write_via_syscall() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    let sc = ctx.syscall();

    // Write
    let data = b"hello vmo syscall";
    sc.sys_vmo_write(h, UserInPtr::from(data.as_ptr() as usize), 0, data.len())
        .unwrap();

    // Read
    let mut buf = [0u8; 32];
    sc.sys_vmo_read(
        h,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        data.len(),
    )
    .unwrap();
    assert_eq!(&buf[..data.len()], data);
}

/// C++: TEST(VmoTestCase, ReadWriteRange) — syscall level
/// Read/write past end returns OUT_OF_RANGE.
#[test]
fn read_write_out_of_range() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    let sc = ctx.syscall();

    // Write past end
    let data = vec![0u8; PAGE_SIZE + 1];
    let result = sc.sys_vmo_write(h, UserInPtr::from(data.as_ptr() as usize), 0, data.len());
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);

    // Read past end
    let mut buf = vec![0u8; PAGE_SIZE + 1];
    let result = sc.sys_vmo_read(h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, buf.len());
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(VmoTestCase, SizeAlign) — syscall level
/// VMO size rounds up to page boundary.
#[test]
fn size_align() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    for size in [1u64, 100, PAGE_SIZE as u64 - 1, PAGE_SIZE as u64 + 1] {
        let mut h: HandleValue = 0;
        sc.sys_vmo_create(size, 0, UserOutPtr::from(&mut h as *mut _ as usize))
            .unwrap();

        let mut actual_size: usize = 0;
        sc.sys_vmo_get_size(h, UserOutPtr::from(&mut actual_size as *mut _ as usize))
            .unwrap();

        let expected = size.div_ceil(PAGE_SIZE as u64) as usize * PAGE_SIZE;
        assert_eq!(
            actual_size, expected,
            "size {} should round to {}",
            size, expected
        );
    }
}

/// C++: TEST(VmoTestCase, Resize) — syscall level
/// Resizable VMO can grow and shrink.
#[test]
fn resize_via_syscall() {
    let ctx = TestContext::new();
    // ZX_VMO_RESIZABLE = 1 << 1 = 2
    let h = syscall_vmo_create(&ctx, (PAGE_SIZE * 2) as u64, 2);
    let sc = ctx.syscall();

    // Grow
    sc.sys_vmo_set_size(h, PAGE_SIZE * 4).unwrap();
    let mut size: usize = 0;
    sc.sys_vmo_get_size(h, UserOutPtr::from(&mut size as *mut _ as usize))
        .unwrap();
    assert_eq!(size, PAGE_SIZE * 4);

    // Shrink
    sc.sys_vmo_set_size(h, PAGE_SIZE).unwrap();
    sc.sys_vmo_get_size(h, UserOutPtr::from(&mut size as *mut _ as usize))
        .unwrap();
    assert_eq!(size, PAGE_SIZE);
}

/// C++: TEST(VmoTestCase, NoResize) — syscall level
/// Non-resizable VMO rejects set_size.
#[test]
fn no_resize_via_syscall() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, (PAGE_SIZE * 2) as u64, 0); // not resizable
    let sc = ctx.syscall();

    let result = sc.sys_vmo_set_size(h, PAGE_SIZE * 4);
    assert!(result.is_err());

    // Size unchanged
    let mut size: usize = 0;
    sc.sys_vmo_get_size(h, UserOutPtr::from(&mut size as *mut _ as usize))
        .unwrap();
    assert_eq!(size, PAGE_SIZE * 2);
}

/// C++: TEST(VmoTestCase, OpRange — COMMIT/DECOMMIT) — syscall level
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

/// C++: TEST(VmoTestCase, OpRange — ZERO) — syscall level
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

/// C++: TEST(VmoTestCase, CreateChild — SNAPSHOT) — syscall level
#[test]
fn create_child_snapshot() {
    let ctx = TestContext::new();
    let parent_h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0);
    let sc = ctx.syscall();

    // Write to parent
    sc.sys_vmo_write(parent_h, UserInPtr::from(b"\xAA".as_ptr() as usize), 0, 1)
        .unwrap();

    // Create SNAPSHOT child (options=1 for ZX_VMO_CHILD_SNAPSHOT)
    let mut child_h: HandleValue = 0;
    sc.sys_vmo_create_child(
        parent_h,
        1, // ZX_VMO_CHILD_SNAPSHOT
        0,
        PAGE_SIZE,
        UserOutPtr::from(&mut child_h as *mut _ as usize),
    )
    .unwrap();

    // Child sees parent data
    let mut buf = [0u8; 1];
    sc.sys_vmo_read(child_h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, 1)
        .unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to parent doesn't affect child (COW)
    sc.sys_vmo_write(parent_h, UserInPtr::from(b"\xBB".as_ptr() as usize), 0, 1)
        .unwrap();
    sc.sys_vmo_read(child_h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, 1)
        .unwrap();
    assert_eq!(buf[0], 0xAA); // still old value
}

//! VMO syscall-level tests.
//!
//! Tests through sys_vmo_create, sys_vmo_read, sys_vmo_write,
//! sys_vmo_set_size, sys_vmo_get_size, sys_vmo_op_range,
//! sys_vmo_create_child.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::HandleValue;
use zircon_object::vm::PAGE_SIZE;

/// Helper: create a VMO through the syscall layer.
pub(crate) fn syscall_vmo_create(ctx: &TestContext, size: u64, options: u32) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_vmo_create(size, options, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}
/// C++: TEST(VmoTestCase, StreamSize) — syscall level
/// Get/set stream size via syscall.
#[test]
fn stream_size_via_syscall() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, (PAGE_SIZE * 4) as u64, 0);
    let sc = ctx.syscall();

    // Set stream size
    sc.sys_vmo_set_stream_size(h, PAGE_SIZE * 2).unwrap();

    // Get stream size
    let mut size: usize = 0;
    sc.sys_vmo_get_stream_size(h, UserOutPtr::from(&mut size as *mut _ as usize))
        .unwrap();
    assert_eq!(size, PAGE_SIZE * 2);
}

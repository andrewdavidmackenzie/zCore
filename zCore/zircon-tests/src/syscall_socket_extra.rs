//! Additional zCore-specific tests for syscall_socket functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::ZxError;
use crate::core_tests::syscall_socket::{syscall_socket_create};

#[test]
fn shutdown_write() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Shutdown write on h0 (flag 1 = SHUTDOWN_WRITE)
    sc.sys_socket_shutdown(h0, 1).unwrap();

    // h0 can no longer write
    let data = b"fail";
    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);

    // h1 can still write to h0
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(b"ok".as_ptr() as usize),
        2,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 2);
}


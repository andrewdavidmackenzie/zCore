//! Additional zCore-specific tests for syscall_fifo functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::ZxError;
use crate::core_tests::syscall_fifo::{syscall_fifo_create};

#[test]
fn write_and_read() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    // Write 3 elements
    let data = [1u32.to_ne_bytes(), 2u32.to_ne_bytes(), 3u32.to_ne_bytes()].concat();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        4,
        UserInPtr::from(data.as_ptr() as usize),
        3,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 3);

    // Read 3 elements
    let mut buf = [0u8; 12];
    sc.sys_fifo_read(
        h1,
        4,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        3,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 3);
    assert_eq!(&buf[0..4], &1u32.to_ne_bytes());
    assert_eq!(&buf[4..8], &2u32.to_ne_bytes());
    assert_eq!(&buf[8..12], &3u32.to_ne_bytes());
}

#[test]
fn elem_size_mismatch() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    let data = [0u8; 8];
    let mut actual: usize = 0;
    let result = sc.sys_fifo_write(
        h0,
        8, // wrong elem_size
        UserInPtr::from(data.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

#[test]
fn peer_closed() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    sc.sys_handle_close(h1).unwrap();

    let data = [0u8; 4];
    let mut actual: usize = 0;
    let result = sc.sys_fifo_write(
        h0,
        4,
        UserInPtr::from(data.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);
}


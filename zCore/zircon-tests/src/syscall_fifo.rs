//! FIFO syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// Helper: create a FIFO pair.
fn syscall_fifo_create(
    ctx: &TestContext,
    elem_count: usize,
    elem_size: usize,
) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_fifo_create(
        elem_count,
        elem_size,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    (h0, h1)
}

/// C++: TEST(FifoTest, EmptyQueueReturnsErrShouldWait) — syscall level
#[test]
fn empty_queue_should_wait() {
    let ctx = TestContext::new();
    let (_h0, h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    let mut buf = [0u8; 4];
    let mut actual: usize = 0;
    let result = sc.sys_fifo_read(
        h1,
        4,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(FifoTest, WriteAndRead) — syscall level
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

/// C++: TEST(FifoTest, ElemSizeMismatch) — syscall level
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

/// C++: TEST(FifoTest, PeerClosed) — syscall level
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

/// C++: TEST(FifoTest, InvalidParametersReturnOutOfRange) — syscall level
#[test]
fn invalid_parameters() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Zero count
    let result = sc.sys_fifo_create(
        0,
        4,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);

    // Zero elem_size
    let result = sc.sys_fifo_create(
        16,
        0,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

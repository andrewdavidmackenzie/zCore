//! FIFO syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// Helper: create a FIFO pair.
pub(crate) fn syscall_fifo_create(
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
/// C++: TEST(FifoTest, WriteNoWriteRights) — syscall level
#[test]
fn write_no_write_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (f0, _f1) = zircon_object::ipc::Fifo::create(16, 4);
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        f0,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    let data = [0u8; 4];
    let mut actual: usize = 0;
    let result = sc.sys_fifo_write(
        h0,
        4,
        UserInPtr::from(data.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(FifoTest, ReadNoReadRights) — syscall level
#[test]
fn read_no_read_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (f0, _f1) = zircon_object::ipc::Fifo::create(16, 4);
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        f0,
        zircon_object::object::Rights::WRITE, // no READ
    ));

    let mut buf = [0u8; 4];
    let mut actual: usize = 0;
    let result = sc.sys_fifo_read(
        h0,
        4,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

// ── Batch-ported FifoTest tests from fifo.cc ──

/// C++: TEST(FifoTest, ReadAndWriteValidatesSizeAndElementCount)
#[test]
fn read_and_write_validates_size_and_element_count() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    let data = [0u8; 64];
    let mut buf = [0u8; 64];
    let mut actual: usize = 0;

    // Not allowed to read or write zero elements
    assert_eq!(
        sc.sys_fifo_read(
            h1,
            8,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            0,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
    assert_eq!(
        sc.sys_fifo_write(
            h0,
            8,
            UserInPtr::from(data.as_ptr() as usize),
            0,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );

    // Element size must match (created with elem_size=8, using 9)
    assert_eq!(
        sc.sys_fifo_read(
            h1,
            9,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            8,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
    assert_eq!(
        sc.sys_fifo_write(
            h0,
            9,
            UserInPtr::from(data.as_ptr() as usize),
            8,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}
/// C++: TEST(FifoTest, ReadWriteNullActualCount)
/// Null actual_count pointer should be accepted (not written to).
#[test]
fn read_write_null_actual_count() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    let data = 1234u64.to_ne_bytes();
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        1,
        UserOutPtr::from(0usize), // null actual_count
    )
    .unwrap();

    let mut buf = [0u8; 8];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        1,
        UserOutPtr::from(0usize), // null actual_count
    )
    .unwrap();
    assert_eq!(u64::from_ne_bytes(buf), 1234);
}
/// C++: TEST(FifoTest, WriteToClosedPeerReturnsPeerClosed)
#[test]
fn write_to_closed_peer_returns_peer_closed() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    // Close the peer
    sc.sys_handle_close(h1).unwrap();

    let data = [0u8; 8];
    let mut actual: usize = 0;
    assert_eq!(
        sc.sys_fifo_write(
            h0,
            8,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

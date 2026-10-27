//! Socket syscall-level tests.
//!
//! Tests through sys_socket_create, sys_socket_write, sys_socket_read,
//! sys_socket_shutdown, sys_socket_set_disposition.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// Helper: create a socket pair through the syscall layer.
pub(crate) fn syscall_socket_create(ctx: &TestContext, options: u32) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_socket_create(
        options,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    (h0, h1)
}
/// C++: TEST(SocketTest, WriteRights) — syscall level
/// Writing without WRITE right returns ACCESS_DENIED.
#[test]
fn write_without_write_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (s0, _s1) = zircon_object::ipc::Socket::create(0).unwrap();
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        s0,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"test".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SocketTest, ReadRights) — syscall level
/// Reading without READ right returns ACCESS_DENIED.
#[test]
fn read_without_read_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (s0, _s1) = zircon_object::ipc::Socket::create(0).unwrap();
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        s0,
        zircon_object::object::Rights::WRITE, // no READ
    ));

    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}
/// C++: TEST(SocketTest, WriteWrongType) — syscall level
/// Writing to a non-socket handle returns WRONG_TYPE.
#[test]
fn write_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h,
        0,
        UserInPtr::from(b"test".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(SocketTest, ReadWrongType) — syscall level
#[test]
fn read_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(SocketTest, DisableWriteLocal) — syscall level
#[test]
fn disable_write_local() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Disable write on h0 (disposition=1 = WRITE_DISABLED)
    sc.sys_socket_set_disposition(h0, 1, 0).unwrap();

    // h0 can no longer write
    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"fail".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);

    // h1 can still write
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(b"ok".as_ptr() as usize),
        2,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
}

/// C++: TEST(SocketTest, DisableWritePeer) — syscall level
#[test]
fn disable_write_peer() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Disable write on peer (disposition_peer=1)
    sc.sys_socket_set_disposition(h0, 0, 1).unwrap();

    // h1 can no longer write
    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(b"fail".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);

    // h0 can still write
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"ok".as_ptr() as usize),
        2,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
}
/// C++: TEST(SocketTest, InvalidHandles) — syscall level
#[test]
fn invalid_handles() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Read from invalid handle
    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    assert_eq!(
        sc.sys_socket_read(
            0,
            0,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            buf.len(),
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Write to invalid handle
    assert_eq!(
        sc.sys_socket_write(
            0,
            0,
            UserInPtr::from(b"test".as_ptr() as usize),
            4,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Set disposition on invalid handle
    assert_eq!(
        sc.sys_socket_set_disposition(0, 0, 0).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(SocketTest, StreamReadIntoBadBuffer) — syscall level
/// Reading into a null pointer with non-zero count returns INVALID_ARGS.
#[test]
fn stream_read_into_bad_buffer() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Write some data so read has something to return
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"data".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Read into null buffer with non-zero count
    let result = sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(0usize), // null buffer
        16,                       // non-zero count
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, StreamWriteBadBufferIntoExistingBufferReturnsInvalidArgs) — syscall level
/// Writing from a null pointer with non-zero count returns INVALID_ARGS.
#[test]
fn stream_write_bad_buffer_into_existing_buffer_returns_invalid_args() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(0usize), // null buffer
        10,                      // non-zero count
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, WriteFromBadBuffer) — syscall level
/// Writing from a null pointer with non-zero count returns INVALID_ARGS.
#[test]
fn write_from_bad_buffer() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(0usize), // null/bad buffer
        4,                       // non-zero count
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, WriteFromNullBuffer) — syscall level
/// Writing from a null pointer with non-zero count returns INVALID_ARGS.
#[test]
fn write_from_null_buffer() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(0usize), // null
        8,                       // non-zero
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, WriteFromPartialBadBuffer) — syscall level
/// Writing from a null pointer with any non-zero count returns INVALID_ARGS.
#[test]
fn write_from_partial_bad_buffer() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(0usize), // null
        1,                       // even 1 byte from null is invalid
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, ReadIntoNullBuffer) — syscall level
/// Reading into a null buffer with non-zero count returns INVALID_ARGS.
#[test]
fn read_into_null_buffer() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Write data so read has something
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"hello".as_ptr() as usize),
        5,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Read into null buffer
    let result = sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(0usize), // null
        32,                       // non-zero
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}
// ── Batch 3: Syscall-level ports from socket.cc (13 missing tests) ──

/// C++: TEST(SocketTest, DatagramWriteBadBufferReturnsInvalidArgs) — syscall level
/// Writing from a bad (null) buffer pointer on a datagram socket
/// returns INVALID_ARGS.
#[test]
fn datagram_write_bad_buffer_returns_invalid_args() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 1); // DATAGRAM
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(0usize), // null/bad buffer
        64,                      // non-zero count
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, GetPropertyRxThresholdBufferTooSmallReturnsBufferTooSmall) — syscall level
/// get_property with a buffer smaller than sizeof(size_t) returns
/// BUFFER_TOO_SMALL.
#[test]
fn get_property_rx_threshold_buffer_too_small_returns_buffer_too_small() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // ZX_PROP_SOCKET_RX_THRESHOLD = 12
    // Provide only 4 bytes (u32) instead of 8 bytes (usize/u64)
    let mut val: u32 = 0;
    let result = sc.sys_object_get_property(
        h0,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &mut val as *mut _ as usize,
        core::mem::size_of::<u32>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(SocketTest, GetPropertyTxThresholdBufferTooSmallReturnsBufferTooSmall) — syscall level
#[test]
fn get_property_tx_threshold_buffer_too_small_returns_buffer_too_small() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // ZX_PROP_SOCKET_TX_THRESHOLD = 13
    let mut val: u32 = 0;
    let result = sc.sys_object_get_property(
        h0,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        &mut val as *mut _ as usize,
        core::mem::size_of::<u32>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(SocketTest, GetPropertyRxThresholdWrongTypeReturnsWrongType) — syscall level
/// Calling get_property(ZX_PROP_SOCKET_RX_THRESHOLD) on a VMO returns
/// WRONG_TYPE.
#[test]
fn get_property_rx_threshold_wrong_type_returns_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let mut val: u64 = 0;
    let result = sc.sys_object_get_property(
        h,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &mut val as *mut _ as usize,
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(SocketTest, GetPropertyTxThresholdWrongTypeReturnsWrongType) — syscall level
#[test]
fn get_property_tx_threshold_wrong_type_returns_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let mut val: u64 = 0;
    let result = sc.sys_object_get_property(
        h,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        &mut val as *mut _ as usize,
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}
/// C++: TEST(SocketTest, SetPropertyRxThresholdBadBufferReturnsInvalidArgs) — syscall level
/// set_property with a bad buffer pointer returns INVALID_ARGS.
#[test]
fn set_property_rx_threshold_bad_buffer_returns_invalid_args() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // ZX_PROP_SOCKET_RX_THRESHOLD = 12
    let result = sc.sys_object_set_property(
        h0,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        0,  // null/bad buffer
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, SetPropertyRxThresholdBufferTooSmallReturnsBufferTooSmall) — syscall level
#[test]
fn set_property_rx_threshold_buffer_too_small_returns_buffer_too_small() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // ZX_PROP_SOCKET_RX_THRESHOLD = 12
    let val: u32 = 0;
    let result = sc.sys_object_set_property(
        h0,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &val as *const _ as usize,
        core::mem::size_of::<u32>(), // too small
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(SocketTest, SetPropertyRxThresholdWrongTypeReturnsWrongType) — syscall level
#[test]
fn set_property_rx_threshold_wrong_type_returns_wrong_type() {
    let ctx = TestContext::new();
    let mut sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let val: u64 = 0;
    let result = sc.sys_object_set_property(
        h,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &val as *const _ as usize,
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(SocketTest, SetPropertyTxThresholdBadBufferReturnsInvalidArgs) — syscall level
#[test]
fn set_property_tx_threshold_bad_buffer_returns_invalid_args() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // ZX_PROP_SOCKET_TX_THRESHOLD = 13
    let result = sc.sys_object_set_property(
        h0,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        0,  // null/bad buffer
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, SetPropertyTxThresholdBufferTooSmallReturnsBufferTooSmall) — syscall level
#[test]
fn set_property_tx_threshold_buffer_too_small_returns_buffer_too_small() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // ZX_PROP_SOCKET_TX_THRESHOLD = 13
    let val: u32 = 0;
    let result = sc.sys_object_set_property(
        h0,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        &val as *const _ as usize,
        core::mem::size_of::<u32>(), // too small
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(SocketTest, SetPropertyTxThresholdWrongTypeReturnsWrongType) — syscall level
#[test]
fn set_property_tx_threshold_wrong_type_returns_wrong_type() {
    let ctx = TestContext::new();
    let mut sc = ctx.syscall();

    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    let val: u64 = 0;
    let result = sc.sys_object_set_property(
        h,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        &val as *const _ as usize,
        core::mem::size_of::<u64>(),
    );
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}
// ── Parameterized DatagramSocketBadBufferTest (TEST_P) ──────────────

/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/0)
/// Read from empty datagram socket returns SHOULD_WAIT.
#[test]
fn datagram_socket_bad_buffer_test_read_0() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 1); // 1 = DATAGRAM
    let sc = ctx.syscall();

    let mut buf = [0u8; 64];
    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result, Err(zircon_object::ZxError::SHOULD_WAIT));
}

/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/1)
/// Read from empty datagram socket with zero-length buffer.
#[test]
fn datagram_socket_bad_buffer_test_read_1() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 1); // DATAGRAM
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(0usize),
        0,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result, Err(zircon_object::ZxError::SHOULD_WAIT));
}

/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/2)
/// Read from datagram socket with peer closed.
#[test]
fn datagram_socket_bad_buffer_test_read_2() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 1); // DATAGRAM
    let sc = ctx.syscall();

    // Close the peer.
    sc.sys_handle_close(h1).unwrap();

    let mut buf = [0u8; 64];
    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result, Err(zircon_object::ZxError::PEER_CLOSED));
}

/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/3)
/// Read from datagram socket with data available.
#[test]
fn datagram_socket_bad_buffer_test_read_3() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 1); // DATAGRAM
    let sc = ctx.syscall();

    // Write some data to the peer.
    let data = [0xAAu8; 8];
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 8);

    // Read it back.
    let mut buf = [0u8; 64];
    let mut read_actual: usize = 0;
    sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut read_actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(read_actual, 8);
    assert_eq!(&buf[..8], &data);
}

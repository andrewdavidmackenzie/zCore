//! Socket syscall-level tests.
//!
//! Tests through sys_socket_create, sys_socket_write, sys_socket_read,
//! sys_socket_shutdown, sys_socket_set_disposition.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// Helper: create a socket pair through the syscall layer.
fn syscall_socket_create(ctx: &TestContext, options: u32) -> (HandleValue, HandleValue) {
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

/// C++: TEST(SocketTest, EmptySocketShouldWait) — syscall level
#[test]
fn empty_socket_should_wait() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut buf = [0u8; 16];
    let mut actual: usize = 0;
    let result = sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(SocketTest, WriteReadDataVerify) — syscall level
#[test]
fn write_read_data_verify() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let data = b"hello socket";
    let mut actual_written: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len(),
        UserOutPtr::from(&mut actual_written as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual_written, 12);

    let mut buf = [0u8; 32];
    let mut actual_read: usize = 0;
    sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual_read as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual_read, 12);
    assert_eq!(&buf[..12], b"hello socket");
}

/// C++: TEST(SocketTest, PeerClosedError) — syscall level
#[test]
fn peer_closed_error() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Close h1
    sc.sys_handle_close(h1).unwrap();

    // Write to h0 should fail
    let data = b"fail";
    let mut actual: usize = 0;
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(SocketTest, Shutdown) — syscall level
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

/// C++: TEST(SocketTest, CreateInvalidArgs) — syscall level
#[test]
fn create_invalid_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let result = sc.sys_socket_create(
        0xFF,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, SetDispositionInvalidArgs) — syscall level
#[test]
fn set_disposition_invalid_args() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    assert_eq!(
        sc.sys_socket_set_disposition(h0, 3, 0).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        sc.sys_socket_set_disposition(h0, 0, 3).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(SocketTest, Datagram) — syscall level
#[test]
fn datagram() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 1); // DATAGRAM
    let sc = ctx.syscall();

    // Write two datagrams
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"msg1".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"message2".as_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Read first datagram
    let mut buf = [0u8; 32];
    sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4);
    assert_eq!(&buf[..4], b"msg1");

    // Read second datagram
    sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 8);
    assert_eq!(&buf[..8], b"message2");
}

/// C++: TEST(SocketTest, PeekingLeavesData) — syscall level
#[test]
fn peeking_leaves_data() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(b"peek".as_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Peek (option 8 = ZX_SOCKET_PEEK)
    let mut buf = [0u8; 16];
    sc.sys_socket_read(
        h1,
        8, // ZX_SOCKET_PEEK
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4);
    assert_eq!(&buf[..4], b"peek");

    // Consume
    sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4);
    assert_eq!(&buf[..4], b"peek");

    // Now empty
    let result = sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

//! Channel syscall-level tests.
//!
//! These tests go through the full syscall dispatch layer
//! (sys_channel_create, sys_channel_write, sys_channel_read),
//! exercising handle table operations, rights checking, and
//! argument validation — the same code path as real Zircon
//! userspace programs.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{HandleValue, KernelObject};
use zircon_object::ZxError;

/// Helper: create a channel pair through the syscall layer.
fn syscall_channel_create(ctx: &TestContext) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let out0 = UserOutPtr::from(&mut h0 as *mut _ as usize);
    let out1 = UserOutPtr::from(&mut h1 as *mut _ as usize);
    sc.sys_channel_create(0, out0, out1).unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    (h0, h1)
}

/// C++: TEST(ChannelTest, CreateIsOkAndEndpointsAreRelated) — syscall level
#[test]
fn create_via_syscall() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_channel_create(&ctx);

    // Both handles are valid and different
    assert_ne!(h0, h1);
}

/// C++: TEST(ChannelTest, CreateInvalidOptionsReturnsInvalidArgs) — syscall level
#[test]
fn create_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let out0 = UserOutPtr::from(&mut h0 as *mut _ as usize);
    let out1 = UserOutPtr::from(&mut h1 as *mut _ as usize);
    let result = sc.sys_channel_create(1, out0, out1);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(ChannelTest, WriteAndRead) — syscall level
/// Write data through syscall, read it back through syscall.
#[test]
fn write_and_read_via_syscall() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Write "hello" through h0
    let data = b"hello";
    let user_bytes = UserInPtr::from(data.as_ptr() as usize);
    sc.sys_channel_write(
        h0,
        0,
        user_bytes,
        data.len() as u32,
        UserInPtr::from(0usize),
        0,
    )
    .unwrap();

    // Read from h1
    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h1,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0, // handles pointer (none)
        buf.len() as u32,
        0, // num_handles
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false, // is_etc
    );
    assert!(result.is_ok());
    assert_eq!(actual_bytes, 5);
    assert_eq!(actual_handles, 0);
    assert_eq!(&buf[..5], b"hello");
}

/// C++: TEST(ChannelTest, ReadWhenEmptyReturnsShouldWait) — syscall level
#[test]
fn read_empty_via_syscall() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        buf.len() as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(ChannelTest, WriteConsumesAllHandles) — syscall level
/// The syscall layer enforces the 64-handle limit.
#[test]
fn write_too_many_handles_via_syscall() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Create 65 event handles to exceed ZX_CHANNEL_MAX_MSG_HANDLES (64)
    let mut handles = Vec::new();
    for _ in 0..65 {
        // Create event via kernel object and add to process handle table
        let event = zircon_object::signal::Event::new();
        let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
            event,
            zircon_object::object::Rights::DEFAULT_EVENT,
        ));
        handles.push(h);
    }

    let user_handles = UserInPtr::from(handles.as_ptr() as usize);
    let result = sc.sys_channel_write(
        h0,
        0,
        UserInPtr::from(0usize),
        0,
        user_handles,
        handles.len() as u32,
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(ChannelTest, ReadWhenEmptyAndClosedReturnsPeerClosed) — syscall level
#[test]
fn read_closed_peer_via_syscall() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Close h1 via syscall
    sc.sys_handle_close(h1).unwrap();

    // Read from h0 — peer is closed
    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        buf.len() as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(ChannelTest, WriteWithHandleTransfer) — syscall level
/// Write a handle through a channel, read it back on the other end.
#[test]
fn write_handle_transfer_via_syscall() {
    let ctx = TestContext::new();
    let (ch0, ch1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Create an event and get its handle
    let event = zircon_object::signal::Event::new();
    let event_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    // Write the event handle through ch0
    let handles_to_write = [event_handle];
    let user_handles = UserInPtr::from(handles_to_write.as_ptr() as usize);
    sc.sys_channel_write(ch0, 0, UserInPtr::from(0usize), 0, user_handles, 1)
        .unwrap();

    // The event handle should be consumed from the sender's handle table
    assert!(ctx
        .proc
        .get_object::<zircon_object::signal::Event>(event_handle)
        .is_err());

    // Read the handle from ch1
    let mut recv_handles = [0u32; 1];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    sc.sys_channel_read(
        ch1,
        0,
        UserOutPtr::from(0usize), // no data
        recv_handles.as_mut_ptr() as usize,
        0,
        1,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    )
    .unwrap();
    assert_eq!(actual_handles, 1);

    // The received handle should resolve to the same event object
    let received_event = ctx
        .proc
        .get_object::<zircon_object::signal::Event>(recv_handles[0])
        .unwrap();
    assert_eq!(received_event.id(), event.id());
}

/// C++: TEST(ChannelTest, ReadWithoutReadRightReturnsAccessDenied) — syscall level
#[test]
fn read_without_read_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create channel and add handle without READ right
    let (ch0, ch1) = zircon_object::ipc::Channel::create();
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::WRITE, // no READ
    ));
    let _h1 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch1,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h0,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        buf.len() as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(ChannelTest, WriteWithoutWriteRightReturnsAccessDenied) — syscall level
#[test]
fn write_without_write_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create channel and add handle without WRITE right
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let h0 = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::READ, // no WRITE
    ));

    let data = b"test";
    let result = sc.sys_channel_write(
        h0,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len() as u32,
        UserInPtr::from(0usize),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(ChannelTest, WriteDataTooLargeReturnsOutOfRange) — syscall level
/// Data exceeding ZX_CHANNEL_MAX_MSG_BYTES (65536) is rejected.
#[test]
fn write_data_too_large_via_syscall() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let big_data = vec![0u8; 65537]; // > 64 KiB
    let user_bytes = UserInPtr::from(big_data.as_ptr() as usize);
    let result = sc.sys_channel_write(
        h0,
        0,
        user_bytes,
        big_data.len() as u32,
        UserInPtr::from(0usize),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

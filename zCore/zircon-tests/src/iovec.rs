//! IOVec channel tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/channel-iovec/iovec.cc.
//! Tests verify iovec-based channel write and call with various edge cases.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// ZX_CHANNEL_WRITE_USE_IOVEC
const USE_IOVEC: u32 = 2;

/// Helper: ChannelIovec ABI layout — [buffer: usize, capacity: u32, reserved: u32].
/// Total size depends on pointer width; on 64-bit: 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ChannelIovec {
    buffer: usize,
    capacity: u32,
    reserved: u32,
}

/// Helper: create a channel pair through the syscall layer.
fn syscall_channel_create(ctx: &TestContext) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_channel_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    (h0, h1)
}

// ────────────────────────── Write tests ──────────────────────────

/// C++: TEST(IOVecTest, WriteZeroIovecs)
#[test]
fn write_zero_iovecs() {
    let ctx = TestContext::new();
    let (write_h, read_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Write with USE_IOVEC, 0 iovecs, no handles
    sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(0usize), // null iovecs pointer, count=0
        0,
        UserInPtr::from(0usize),
        0,
    )
    .unwrap();

    // Read should yield 0 bytes, 0 handles
    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    sc.sys_channel_read(
        read_h,
        0,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        buf.len() as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    )
    .unwrap();
    assert_eq!(actual_bytes, 0);
    assert_eq!(actual_handles, 0);
}

/// Helper: write N iovecs of M bytes each, then read and verify.
fn write_read_iovecs<const N: usize, const M: usize>(ctx: &TestContext) {
    let (write_h, read_h) = syscall_channel_create(ctx);
    let sc = ctx.syscall();

    // Build input buffers
    let mut inputs = [[0u8; M]; N];
    for i in 0..N {
        let fill = ((i + (i / 256)) & 0xFF) as u8;
        inputs[i] = [fill; M];
    }

    // Build iovec array
    let iovecs: Vec<ChannelIovec> = inputs
        .iter()
        .map(|buf| ChannelIovec {
            buffer: buf.as_ptr() as usize,
            capacity: M as u32,
            reserved: 0,
        })
        .collect();

    // Write using iovec
    sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        N as u32, // num_iovecs
        UserInPtr::from(0usize),
        0,
    )
    .unwrap();

    // Read
    let total = N * M;
    let mut output = vec![0u8; total];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    sc.sys_channel_read(
        read_h,
        0,
        UserOutPtr::from(output.as_mut_ptr() as usize),
        0,
        total as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    )
    .unwrap();
    assert_eq!(actual_bytes as usize, total);
    assert_eq!(actual_handles, 0);

    for i in 0..N {
        assert_eq!(
            &output[i * M..(i + 1) * M],
            &inputs[i],
            "iovec {} mismatch",
            i
        );
    }
}

/// C++: TEST(IOVecTest, WriteFewLargeIovecs)
/// Test writing a small number of iovecs, which fit within the kernel stack buffer.
#[test]
fn write_few_large_iovecs() {
    let ctx = TestContext::new();
    write_read_iovecs::<3, 8000>(&ctx);
}

/// C++: TEST(IOVecTest, WriteManySmallIovecs)
/// Test writing a large number of iovecs, that don't fit within the stack buffer.
#[test]
fn write_many_small_iovecs() {
    let ctx = TestContext::new();
    write_read_iovecs::<1000, 10>(&ctx);
}

/// C++: TEST(IOVecTest, WriteWithHandle)
#[test]
fn write_with_handle() {
    let ctx = TestContext::new();
    let (write_h, read_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Build iovec with 8 bytes
    let in_bytes = [0u8; 8];
    let iovecs = [ChannelIovec {
        buffer: in_bytes.as_ptr() as usize,
        capacity: 8,
        reserved: 0,
    }];

    // Create an event handle to transfer
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    let handles_to_write = [event_h];
    sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        1, // 1 iovec
        UserInPtr::from(handles_to_write.as_ptr() as usize),
        1,
    )
    .unwrap();

    // Read back
    let mut out_bytes = [0u8; 8];
    let mut out_handles = [0u32; 1];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    sc.sys_channel_read(
        read_h,
        0,
        UserOutPtr::from(out_bytes.as_mut_ptr() as usize),
        out_handles.as_mut_ptr() as usize,
        8,
        1,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    )
    .unwrap();
    assert_eq!(actual_bytes, 8);
    assert_eq!(actual_handles, 1);
    assert_ne!(out_handles[0], 0);
}

/// C++: TEST(IOVecTest, WriteEtcWithHandle)
#[test]
fn write_etc_with_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create channel via kernel objects
    let (ch0, ch1) = zircon_object::ipc::Channel::create();
    let write_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    let read_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch1,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    // Build iovec with 8 bytes
    let in_bytes = [0u8; 8];
    let iovecs = [ChannelIovec {
        buffer: in_bytes.as_ptr() as usize,
        capacity: 8,
        reserved: 0,
    }];

    // Create an event handle
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));

    // Build handle disposition: [op=MOVE(0), handle, type=EVENT(5), rights=SAME_RIGHTS, result=0]
    let mut disp = [
        0u32,                                              // ZX_HANDLE_OP_MOVE
        event_h,                                           // handle
        5u32,                                              // ZX_OBJ_TYPE_EVENT
        zircon_object::object::Rights::SAME_RIGHTS.bits(), // rights
        0u32,                                              // result
    ];

    sc.sys_channel_write_etc(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        1, // 1 iovec
        hal_impl::user::UserInOutPtr::from(disp.as_mut_ptr() as usize),
        1,
    )
    .unwrap();

    // Read back
    let mut out_bytes = [0u8; 8];
    let mut out_handles = [0u32; 1];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    sc.sys_channel_read(
        read_h,
        0,
        UserOutPtr::from(out_bytes.as_mut_ptr() as usize),
        out_handles.as_mut_ptr() as usize,
        8,
        1,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    )
    .unwrap();
    assert_eq!(actual_bytes, 8);
    assert_eq!(actual_handles, 1);
    assert_ne!(out_handles[0], 0);
}

/// Helper: check that writing too many iovecs or too many bytes returns OUT_OF_RANGE.
fn check_for_out_of_range_write<const N: usize, const M: usize>(ctx: &TestContext) {
    let (write_h, _read_h) = syscall_channel_create(ctx);
    let sc = ctx.syscall();

    let inputs = vec![[0u8; M]; N];
    let iovecs: Vec<ChannelIovec> = inputs
        .iter()
        .map(|buf| ChannelIovec {
            buffer: buf.as_ptr() as usize,
            capacity: M as u32,
            reserved: 0,
        })
        .collect();

    let result = sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        N as u32,
        UserInPtr::from(0usize),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(IOVecTest, WriteTooManyIovecs)
#[test]
fn write_too_many_iovecs() {
    let ctx = TestContext::new();
    check_for_out_of_range_write::<9000, 1>(&ctx);
}

/// C++: TEST(IOVecTest, WriteTooManyBytes)
#[test]
fn write_too_many_bytes() {
    let ctx = TestContext::new();
    check_for_out_of_range_write::<1000, 100>(&ctx);
}

/// C++: TEST(IOVecTest, WriteNonZeroReservedIovec)
#[test]
fn write_non_zero_reserved_iovec() {
    let ctx = TestContext::new();
    let (write_h, _read_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let buffer = [0u8; 256];
    let iovecs = [ChannelIovec {
        buffer: buffer.as_ptr() as usize,
        capacity: 256,
        reserved: 1, // non-zero reserved field
    }];

    let result = sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        1,
        UserInPtr::from(0usize),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(IOVecTest, WriteNullBufferNonnullSize)
#[test]
fn write_null_buffer_nonnull_size() {
    let ctx = TestContext::new();
    let (write_h, _read_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let iovecs = [ChannelIovec {
        buffer: 0,   // null buffer
        capacity: 8, // non-zero capacity
        reserved: 0,
    }];

    let result = sc.sys_channel_write(
        write_h,
        USE_IOVEC,
        UserInPtr::from(iovecs.as_ptr() as usize),
        1,
        UserInPtr::from(0usize),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

// ────────────────────────── Call tests ──────────────────────────
// Channel call requires a response thread to echo the message back.
// In our test environment we don't have async echo servers, so most
// call tests will time out. These are marked #[ignore].

/// Helper struct matching ChannelCallArgs ABI.
#[repr(C)]
#[derive(Debug)]
struct TestChannelCallArgs {
    wr_bytes: usize,
    wr_handles: usize,
    rd_bytes: usize,
    rd_handles: usize,
    wr_num_bytes: u32,
    wr_num_handles: u32,
    rd_num_bytes: u32,
    rd_num_handles: u32,
}

/// C++: TEST(IOVecTest, CallIovecBytesLessThanTxidSize)
/// Call with iovec total bytes < sizeof(zx_txid_t) (4) should return INVALID_ARGS.
#[test]
fn call_iovec_bytes_less_than_txid_size() {
    let ctx = TestContext::new();
    let (client_h, _server_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // 3 bytes — less than txid size (4)
    let inputs = [0u8; 3];
    let iovecs = [ChannelIovec {
        buffer: inputs.as_ptr() as usize,
        capacity: 3,
        reserved: 0,
    }];

    let args = TestChannelCallArgs {
        wr_bytes: iovecs.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 1, // 1 iovec
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = async_std::task::block_on(sc.sys_channel_call_noretry(
        client_h,
        USE_IOVEC,
        zircon_syscall::Deadline::from(0usize),
        UserInPtr::from(&args as *const _ as usize),
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
    ));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(IOVecTest, CallFewLargeIovecs)
/// Call with a small number of large iovecs. Requires echo server → times out.
#[test]
#[ignore = "channel call needs echo server thread (cooperative scheduler cannot provide one)"]
fn call_few_large_iovecs() {
    // Requires an echo server thread to respond to the call.
}

/// C++: TEST(IOVecTest, CallManySmallIovecs)
/// Call with many small iovecs. Requires echo server → times out.
#[test]
#[ignore = "channel call needs echo server thread (cooperative scheduler cannot provide one)"]
fn call_many_small_iovecs() {
    // Requires an echo server thread to respond to the call.
}

/// C++: TEST(IOVecTest, CallWithHandle)
/// Call with iovec data + handle. Requires echo server → times out.
#[test]
#[ignore = "channel call needs echo server thread (cooperative scheduler cannot provide one)"]
fn call_with_handle() {
    // Requires an echo server thread to respond to the call.
}

/// C++: TEST(IOVecTest, CallEtcWithHandle)
/// Call etc with iovec data + handle disposition. Requires echo server → times out.
#[test]
#[ignore = "channel call_etc needs echo server thread (cooperative scheduler cannot provide one)"]
fn call_etc_with_handle() {
    // Requires an echo server thread to respond to the call.
}

/// Helper: check that call with too many iovecs or too many bytes returns OUT_OF_RANGE.
fn check_for_out_of_range_call<const N: usize, const M: usize>(ctx: &TestContext) {
    let sc = ctx.syscall();
    let (ch0, ch1) = zircon_object::ipc::Channel::create();
    let client_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    let _server_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ch1,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    let inputs = vec![[0u8; M]; N];
    let iovecs: Vec<ChannelIovec> = inputs
        .iter()
        .map(|buf| ChannelIovec {
            buffer: buf.as_ptr() as usize,
            capacity: M as u32,
            reserved: 0,
        })
        .collect();

    let args = TestChannelCallArgs {
        wr_bytes: iovecs.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: N as u32,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = async_std::task::block_on(sc.sys_channel_call_etc_noretry(
        client_h,
        USE_IOVEC,
        zircon_syscall::Deadline::from(0usize),
        UserInPtr::from(&args as *const _ as usize),
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
    ));
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(IOVecTest, CallTooManyIovecs)
#[test]
fn call_too_many_iovecs() {
    let ctx = TestContext::new();
    check_for_out_of_range_call::<9000, 1>(&ctx);
}

/// C++: TEST(IOVecTest, CallTooManyBytes)
#[test]
fn call_too_many_bytes() {
    let ctx = TestContext::new();
    check_for_out_of_range_call::<1000, 100>(&ctx);
}

/// C++: TEST(IOVecTest, CallNonZeroReservedIovec)
#[test]
fn call_non_zero_reserved_iovec() {
    let ctx = TestContext::new();
    let (client_h, _server_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let buffer = [0u8; 256];
    let iovecs = [ChannelIovec {
        buffer: buffer.as_ptr() as usize,
        capacity: 256,
        reserved: 1, // non-zero
    }];

    let args = TestChannelCallArgs {
        wr_bytes: iovecs.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 1,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = async_std::task::block_on(sc.sys_channel_call_noretry(
        client_h,
        USE_IOVEC,
        zircon_syscall::Deadline::from(0usize),
        UserInPtr::from(&args as *const _ as usize),
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
    ));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(IOVecTest, CallNullBufferNonnullSize)
#[test]
fn call_null_buffer_nonnull_size() {
    let ctx = TestContext::new();
    let (client_h, _server_h) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    let iovecs = [ChannelIovec {
        buffer: 0,   // null
        capacity: 8, // non-zero
        reserved: 0,
    }];

    let args = TestChannelCallArgs {
        wr_bytes: iovecs.as_ptr() as usize,
        wr_handles: 0,
        rd_bytes: 0,
        rd_handles: 0,
        wr_num_bytes: 1,
        wr_num_handles: 0,
        rd_num_bytes: 0,
        rd_num_handles: 0,
    };

    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = async_std::task::block_on(sc.sys_channel_call_noretry(
        client_h,
        USE_IOVEC,
        zircon_syscall::Deadline::from(0usize),
        UserInPtr::from(&args as *const _ as usize),
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
    ));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

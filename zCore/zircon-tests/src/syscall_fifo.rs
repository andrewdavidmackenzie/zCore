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

/// C++: TEST(FifoTest, CreateZeroSize) — syscall level
#[test]
fn create_zero_size() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Zero count
    assert_eq!(
        sc.sys_fifo_create(
            0,
            4,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );

    // Zero elem_size
    assert_eq!(
        sc.sys_fifo_create(
            16,
            0,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(FifoTest, EndpointCloseSignalsPeerClosed) — syscall level
#[test]
fn endpoint_close_signals_peer_closed() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    // Write data before closing
    let data = [42u32.to_ne_bytes()].concat();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        4,
        UserInPtr::from(data.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Close h0
    sc.sys_handle_close(h0).unwrap();

    // h1 can still read the buffered data
    let mut buf = [0u8; 4];
    sc.sys_fifo_read(
        h1,
        4,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(u32::from_ne_bytes(buf), 42);

    // After draining, read returns PEER_CLOSED
    assert_eq!(
        sc.sys_fifo_read(
            h1,
            4,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(FifoTest, WritePeerClosedReturnsPeerClosed) — syscall level
#[test]
fn write_peer_closed_returns_peer_closed() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 16, 4);
    let sc = ctx.syscall();

    sc.sys_handle_close(h1).unwrap();

    let data = [0u8; 4];
    let mut actual: usize = 0;
    assert_eq!(
        sc.sys_fifo_write(
            h0,
            4,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::PEER_CLOSED
    );
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

/// C++: TEST(FifoTest, ReadWrite)
/// Write elements and read them back, verifying order and values.
#[test]
fn read_write() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    let data: Vec<u8> = (1u64..=8).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 8);

    let mut buf = vec![0u8; 64];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 8);
    assert_eq!(&buf[..64], &data[..]);
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

/// C++: TEST(FifoTest, ReadWriteRange)
/// Fill FIFO, read half, write more (wrapping), read across wrap boundary.
#[test]
fn read_write_range() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    // Write 8 elements to fill the FIFO
    let data: Vec<u8> = (1u64..=8).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 8);

    // Read 4 elements
    let mut buf = vec![0u8; 32];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4);

    // Write 2 more (wrapping to front)
    let more: Vec<u8> = [9u64, 10u64].iter().flat_map(|x| x.to_ne_bytes()).collect();
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(more.as_ptr() as usize),
        2,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 2);

    // Read across the wrap — should get 6 elements (5..10)
    let mut buf2 = vec![0u8; 64];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf2.as_mut_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 6);

    let expected: Vec<u64> = vec![5, 6, 7, 8, 9, 10];
    for (i, &exp) in expected.iter().enumerate() {
        let val = u64::from_ne_bytes(buf2[i * 8..(i + 1) * 8].try_into().unwrap());
        assert_eq!(val, exp, "element {} mismatch", i);
    }
}

/// C++: TEST(FifoTest, ReadErrors)
/// Reading from a bad handle or wrong type returns proper errors.
#[test]
fn read_errors() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut buf = [0u8; 8];
    let mut actual: usize = 0;

    // Bad handle (0 is never valid)
    assert_eq!(
        sc.sys_fifo_read(
            0,
            8,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Wrong type — use an event handle
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));
    assert_eq!(
        sc.sys_fifo_read(
            event_h,
            8,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::WRONG_TYPE
    );
}

/// C++: TEST(FifoTest, ReadInvalidHandle)
#[test]
fn read_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut buf = [0u8; 8];
    let mut actual: usize = 0;
    assert_eq!(
        sc.sys_fifo_read(
            0xDEAD_BEEF,
            8,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(FifoTest, WriteErrors)
/// Writing to a bad handle or wrong type returns proper errors.
#[test]
fn write_errors() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let data = [0u8; 8];
    let mut actual: usize = 0;

    // Bad handle
    assert_eq!(
        sc.sys_fifo_write(
            0,
            8,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Wrong type — use an event handle
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event,
        zircon_object::object::Rights::DEFAULT_EVENT,
    ));
    assert_eq!(
        sc.sys_fifo_write(
            event_h,
            8,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::WRONG_TYPE
    );
}

/// C++: TEST(FifoTest, WriteInvalidHandle)
#[test]
fn write_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let data = [0u8; 8];
    let mut actual: usize = 0;
    assert_eq!(
        sc.sys_fifo_write(
            0xDEAD_BEEF,
            8,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(FifoTest, PartialWriteQueuesElementsThatFit)
#[test]
fn partial_write_queues_elements_that_fit() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    // Fill with 5 elements
    let data: Vec<u8> = (1u64..=8).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        5,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 5);

    // Try to write 5 more — only 3 should fit
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        5,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 3);
}

/// C++: TEST(FifoTest, CreateMaxBufferBoundary)
#[test]
fn create_max_buffer_boundary() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Exact maximum: 512 * 8 = 4096 bytes
    sc.sys_fifo_create(
        512,
        8,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Over maximum: 513 * 8 = 4104 bytes
    assert_eq!(
        sc.sys_fifo_create(
            513,
            8,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );

    // Over maximum: 512 * 9 = 4608 bytes
    assert_eq!(
        sc.sys_fifo_create(
            512,
            9,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(FifoTest, CreateArithmeticOverflow)
#[test]
fn create_arithmetic_overflow() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Very large values that would overflow
    assert_eq!(
        sc.sys_fifo_create(
            u32::MAX as usize,
            2,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
    assert_eq!(
        sc.sys_fifo_create(
            2,
            u32::MAX as usize,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
    assert_eq!(
        sc.sys_fifo_create(
            1 << 20,
            1 << 20,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(FifoTest, CreateMinAndMaxSizes)
#[test]
fn create_min_and_max_sizes() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Minimum valid sizes
    sc.sys_fifo_create(
        1,
        1,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Maximum buffer: count * elem_size == 4096
    sc.sys_fifo_create(
        4096,
        1,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    sc.sys_fifo_create(
        1,
        4096,
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Exceeding maximum (4097 bytes)
    assert_eq!(
        sc.sys_fifo_create(
            4097,
            1,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
    assert_eq!(
        sc.sys_fifo_create(
            1,
            4097,
            0,
            UserOutPtr::from(&mut h0 as *mut _ as usize),
            UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
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

/// C++: TEST(FifoTest, ReadAndWriteWithMultipleSizes)
/// Write and read with various element sizes.
#[test]
fn read_and_write_with_multiple_sizes() {
    let ctx = TestContext::new();

    for elem_size in [1, 2, 4, 8, 16, 32] {
        let (h0, h1) = syscall_fifo_create(&ctx, 8, elem_size);
        let sc = ctx.syscall();

        let data = vec![0xAAu8; elem_size * 3];
        let mut actual: usize = 0;
        sc.sys_fifo_write(
            h0,
            elem_size,
            UserInPtr::from(data.as_ptr() as usize),
            3,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap();
        assert_eq!(actual, 3);

        let mut buf = vec![0u8; elem_size * 3];
        sc.sys_fifo_read(
            h1,
            elem_size,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            3,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap();
        assert_eq!(actual, 3);
        assert_eq!(buf, data);
    }
}

/// C++: TEST(FifoTest, ReadWriteBadLen)
/// Element count 0 returns OUT_OF_RANGE.
#[test]
fn read_write_bad_len() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    let data = [0u8; 8];
    let mut buf = [0u8; 8];
    let mut actual: usize = 0;

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
}

/// C++: TEST(FifoTest, ReadWriteShrinkRace)
/// Close one endpoint while the other has data. Read should return data
/// before returning PEER_CLOSED.
#[test]
fn read_write_shrink_race() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    // Write 3 elements
    let data: Vec<u8> = (1u64..=3).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        3,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 3);

    // Close writer
    sc.sys_handle_close(h0).unwrap();

    // Read should still return buffered data
    let mut buf = vec![0u8; 24];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        3,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 3);

    // After draining, read should return PEER_CLOSED
    assert_eq!(
        sc.sys_fifo_read(
            h1,
            8,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(FifoTest, SmallReportBuffer)
/// Reading with a buffer smaller than requested count should return
/// only the available elements.
#[test]
fn small_report_buffer() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 8, 8);
    let sc = ctx.syscall();

    // Write 5 elements
    let data: Vec<u8> = (1u64..=5).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        5,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Read only 2
    let mut buf = vec![0u8; 16];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        2,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 2);
    assert_eq!(u64::from_ne_bytes(buf[0..8].try_into().unwrap()), 1);
    assert_eq!(u64::from_ne_bytes(buf[8..16].try_into().unwrap()), 2);
}

/// C++: TEST(FifoTest, LargeWriteBatched)
/// Write more elements than capacity in one call — only capacity elements fit.
#[test]
fn large_write_batched() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_fifo_create(&ctx, 4, 8);
    let sc = ctx.syscall();

    // Write 8 elements into capacity-4 FIFO — partial write
    let data: Vec<u8> = (1u64..=8).flat_map(|x| x.to_ne_bytes()).collect();
    let mut actual: usize = 0;
    sc.sys_fifo_write(
        h0,
        8,
        UserInPtr::from(data.as_ptr() as usize),
        8,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4); // only 4 fit

    // Read all 4
    let mut buf = vec![0u8; 32];
    sc.sys_fifo_read(
        h1,
        8,
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        4,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 4);
    for i in 0..4u64 {
        let val = u64::from_ne_bytes(
            buf[i as usize * 8..(i as usize + 1) * 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(val, i + 1);
    }
}

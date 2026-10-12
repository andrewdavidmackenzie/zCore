//! FIFO syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Fifo;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

#[test]
fn fifo_create() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4); // 16 elements, 4 bytes each
    assert_ne!(f0.id(), f1.id());
}

#[test]
fn fifo_write_read() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    // Write 3 elements of 4 bytes each
    let data = [1u32.to_ne_bytes(), 2u32.to_ne_bytes(), 3u32.to_ne_bytes()].concat();
    let n = f0.write(4, &data, 3).unwrap();
    assert_eq!(n, 3);

    // Read them back
    let mut buf = vec![0u8; 12];
    let n = f1.read(4, &mut buf, 3).unwrap();
    assert_eq!(n, 3);
    assert_eq!(&buf[0..4], &1u32.to_ne_bytes());
    assert_eq!(&buf[4..8], &2u32.to_ne_bytes());
    assert_eq!(&buf[8..12], &3u32.to_ne_bytes());
}

#[test]
fn fifo_read_empty() {
    let _ctx = TestContext::new();
    let (_f0, f1) = Fifo::create(16, 4);

    let mut buf = vec![0u8; 4];
    let err = f1.read(4, &mut buf, 1).unwrap_err();
    assert_eq!(err, ZxError::SHOULD_WAIT);
}

#[test]
fn fifo_peer_closed() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);
    drop(f1);

    let err = f0.write(4, &[0u8; 4], 1).unwrap_err();
    assert_eq!(err, ZxError::PEER_CLOSED);
}

#[test]
fn fifo_elem_size_mismatch() {
    let _ctx = TestContext::new();
    let (f0, _f1) = Fifo::create(16, 4);

    // Wrong element size should fail
    let err = f0.write(8, &[0u8; 8], 1).unwrap_err();
    assert_eq!(err, ZxError::OUT_OF_RANGE);
}

#[test]
fn fifo_read_elem_size_mismatch() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    // Write with correct size
    f0.write(4, &[0u8; 4], 1).unwrap();

    // Read with wrong size
    let mut buf = [0u8; 8];
    assert_eq!(f1.read(8, &mut buf, 1).unwrap_err(), ZxError::OUT_OF_RANGE);
}

#[test]
fn fifo_writable_signal() {
    let _ctx = TestContext::new();
    let (f0, _f1) = Fifo::create(4, 4); // only 4 elements

    assert!(f0.signal().contains(Signal::WRITABLE));

    // Fill the FIFO
    for _ in 0..4 {
        f0.write(4, &[0u8; 4], 1).unwrap();
    }

    // Should no longer be writable (peer's queue is full)
    assert!(!f0.signal().contains(Signal::WRITABLE));
}

#[test]
fn fifo_readable_signal() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    assert!(!f1.signal().contains(Signal::READABLE));

    f0.write(4, &[0u8; 4], 1).unwrap();
    assert!(f1.signal().contains(Signal::READABLE));

    let mut buf = [0u8; 4];
    f1.read(4, &mut buf, 1).unwrap();
    assert!(!f1.signal().contains(Signal::READABLE));
}

#[test]
fn fifo_peer_closed_signal() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    assert!(!f0.signal().contains(Signal::PEER_CLOSED));
    drop(f1);
    assert!(f0.signal().contains(Signal::PEER_CLOSED));
    assert!(!f0.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(FifoTest, PartialWrite)
#[test]
fn fifo_partial_write() {
    let _ctx = TestContext::new();
    let (f0, _f1) = Fifo::create(4, 4); // capacity: 4 elements

    // Write 3 elements
    let data = [0u8; 12];
    let n = f0.write(4, &data, 3).unwrap();
    assert_eq!(n, 3);

    // Try to write 3 more (only 1 fits)
    let n = f0.write(4, &data, 3).unwrap();
    assert_eq!(n, 1);
}

#[test]
fn fifo_read_after_peer_closed() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    // Write data then close peer
    f0.write(4, &42u32.to_ne_bytes(), 1).unwrap();
    drop(f0);

    // Should still be able to read buffered data
    let mut buf = [0u8; 4];
    let n = f1.read(4, &mut buf, 1).unwrap();
    assert_eq!(n, 1);
    assert_eq!(u32::from_ne_bytes(buf), 42);

    // After draining, should get PEER_CLOSED
    assert_eq!(f1.read(4, &mut buf, 1).unwrap_err(), ZxError::PEER_CLOSED);
}

#[test]
fn fifo_related_koid() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    assert_eq!(f0.related_koid(), f1.id());
    assert_eq!(f1.related_koid(), f0.id());

    drop(f1);
    assert_eq!(f0.related_koid(), 0);
}

/// C++: TEST(FifoTest, FifoOrder)
#[test]
fn fifo_order() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    for i in 0u32..8 {
        f0.write(4, &i.to_ne_bytes(), 1).unwrap();
    }

    for i in 0u32..8 {
        let mut buf = [0u8; 4];
        f1.read(4, &mut buf, 1).unwrap();
        assert_eq!(u32::from_ne_bytes(buf), i);
    }
}

// -- Faithful 1:1 ports from FifoTest (fifo.cc) --

/// C++: TEST(FifoTest, DequeueSignalsWriteable)
/// Full signal lifecycle: write fills, read drains, signal transitions.
#[test]
fn dequeue_signals_writeable() {
    let _ctx = TestContext::new();
    let (fa, fb) = Fifo::create(8, 8); // 8 elements of 8 bytes

    assert!(fa.signal().contains(Signal::WRITABLE));
    assert!(fb.signal().contains(Signal::WRITABLE));

    // Fill the FIFO completely
    let data: Vec<u8> = (1u64..=8).flat_map(|x| x.to_ne_bytes()).collect();
    let n = fa.write(8, &data, 8).unwrap();
    assert_eq!(n, 8);

    // fb should be readable
    assert!(fb.signal().contains(Signal::READABLE));

    // fa should no longer be writable (peer's queue is full)
    assert!(!fa.signal().contains(Signal::WRITABLE));

    // Read half
    let mut buf = [0u8; 32];
    let n = fb.read(8, &mut buf, 4).unwrap();
    assert_eq!(n, 4);

    // fa should be writable again
    assert!(fa.signal().contains(Signal::WRITABLE));

    // Read remaining
    let n = fb.read(8, &mut buf, 4).unwrap();
    assert_eq!(n, 4);

    // fb should no longer be readable
    assert!(!fb.signal().contains(Signal::READABLE));
}

/// C++: TEST(FifoTest, NonPowerOfTwoCountSupported)
/// FIFO with non-power-of-2 count works correctly.
#[test]
fn non_power_of_two_count_supported() {
    let _ctx = TestContext::new();
    let (fa, fb) = Fifo::create(10, 4); // 10 elements

    // Write all 10 elements
    let data: Vec<u8> = (0u32..10).flat_map(|x| x.to_ne_bytes()).collect();
    let n = fa.write(4, &data, 10).unwrap();
    assert_eq!(n, 10);

    // Read all 10 back
    let mut buf = vec![0u8; 40];
    let n = fb.read(4, &mut buf, 10).unwrap();
    assert_eq!(n, 10);

    for i in 0u32..10 {
        let val = u32::from_ne_bytes(
            buf[i as usize * 4..(i as usize + 1) * 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(val, i);
    }
}

/// C++: TEST(FifoTest, SingleElementCapacity)
/// FIFO with capacity=1, signal transitions on single element.
#[test]
fn single_element_capacity() {
    let _ctx = TestContext::new();
    let (fa, fb) = Fifo::create(1, 8);

    assert!(fa.signal().contains(Signal::WRITABLE));
    assert!(!fb.signal().contains(Signal::READABLE));

    // Write one element — fa no longer writable, fb becomes readable
    let data = 42u64.to_ne_bytes();
    fa.write(8, &data, 1).unwrap();
    assert!(!fa.signal().contains(Signal::WRITABLE));
    assert!(fb.signal().contains(Signal::READABLE));

    // Read one element — fa writable again, fb not readable
    let mut buf = [0u8; 8];
    fb.read(8, &mut buf, 1).unwrap();
    assert_eq!(u64::from_ne_bytes(buf), 42);
    assert!(fa.signal().contains(Signal::WRITABLE));
    assert!(!fb.signal().contains(Signal::READABLE));
}

/// C++: TEST(FifoTest, UserSignalsAndSignalPeer)
/// User signals on FIFO endpoints.
#[test]
fn user_signals_and_signal_peer() {
    let _ctx = TestContext::new();
    let (fa, fb) = Fifo::create(8, 4);

    // Set user signal on self
    fa.signal_set(Signal::USER_SIGNAL_0);
    assert!(fa.signal().contains(Signal::USER_SIGNAL_0));
    assert!(!fb.signal().contains(Signal::USER_SIGNAL_0));

    // Signal peer
    fa.peer().unwrap().signal_set(Signal::USER_SIGNAL_1);
    assert!(fb.signal().contains(Signal::USER_SIGNAL_1));
    assert!(!fa.signal().contains(Signal::USER_SIGNAL_1));
}

/// Creating a FIFO with 0 count at the syscall level fails.
/// At the object level, Fifo::create does not validate — this is tested
/// in syscall_fifo.rs (create_zero_size / invalid_parameters).
/// Here we verify the object still works if count=0 (degenerate case).
#[test]
fn zero_size_count() {
    let _ctx = TestContext::new();
    let (fa, _fb) = Fifo::create(0, 4); // zero count

    // Cannot write anything (queue capacity is 0)
    let data = [0u8; 4];
    let err = fa.write(4, &data, 1).unwrap_err();
    // Full queue returns SHOULD_WAIT (no space), same as trying to
    // write into a capacity-0 queue.
    assert!(err == ZxError::SHOULD_WAIT || err == ZxError::OUT_OF_RANGE);
}

/// Creating a FIFO with 0 elem_size at the syscall level fails.
/// At the object level, Fifo::create does not validate — this is tested
/// in syscall_fifo.rs (invalid_parameters).
/// Here we verify the object API returns an error when elem_size=0.
#[test]
fn invalid_elem_size() {
    let _ctx = TestContext::new();
    let (fa, _fb) = Fifo::create(16, 0); // zero elem_size

    // Writing with elem_size=0 should fail
    let err = fa.write(0, &[], 0).unwrap_err();
    assert_eq!(err, ZxError::OUT_OF_RANGE);
}

/// Reading from a FIFO whose peer is closed and buffer is empty
/// returns PEER_CLOSED immediately.
#[test]
fn peer_closed_read_returns_peer_closed() {
    let _ctx = TestContext::new();
    let (fa, fb) = Fifo::create(16, 4);
    drop(fa);

    // No data was written — reading should get PEER_CLOSED
    let mut buf = [0u8; 4];
    assert_eq!(fb.read(4, &mut buf, 1).unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(FifoTest, DefaultRightsAndType)
/// Create a FIFO pair, verify type_name and initial signals.
#[test]
fn default_rights_and_type() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    assert_eq!(f0.type_name(), "Fifo");
    assert_eq!(f1.type_name(), "Fifo");

    // Initially both endpoints should be writable, neither readable
    assert!(f0.signal().contains(Signal::WRITABLE));
    assert!(f1.signal().contains(Signal::WRITABLE));
    assert!(!f0.signal().contains(Signal::READABLE));
    assert!(!f1.signal().contains(Signal::READABLE));
}

/// C++: TEST(FifoTest, EndpointsAreRelated)
/// Create a FIFO pair, verify related_koid links the two endpoints.
#[test]
fn endpoints_are_related() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    assert_eq!(f0.related_koid(), f1.id());
    assert_eq!(f1.related_koid(), f0.id());
    assert_ne!(f0.id(), f1.id());
}

/// C++: TEST(FifoTest, FifoOrderIsPreserved)
/// Write N elements, read all back in one call, verify FIFO order.
#[test]
fn fifo_order_is_preserved() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    // Write 8 elements in order
    let data: Vec<u8> = (0u32..8).flat_map(|x| x.to_ne_bytes()).collect();
    let n = f0.write(4, &data, 8).unwrap();
    assert_eq!(n, 8);

    // Read all 8 back in a single call
    let mut buf = vec![0u8; 32];
    let n = f1.read(4, &mut buf, 8).unwrap();
    assert_eq!(n, 8);

    for i in 0u32..8 {
        let val = u32::from_ne_bytes(
            buf[i as usize * 4..(i as usize + 1) * 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(val, i, "element {} should match", i);
    }
}

/// C++: TEST(FifoTest, IndividualReadsPreserveOrder)
/// Write N elements, read one at a time, verify FIFO order.
#[test]
fn individual_reads_preserve_order() {
    let _ctx = TestContext::new();
    let (f0, f1) = Fifo::create(16, 4);

    // Write 8 elements
    let data: Vec<u8> = (0u32..8).flat_map(|x| x.to_ne_bytes()).collect();
    let n = f0.write(4, &data, 8).unwrap();
    assert_eq!(n, 8);

    // Read one at a time
    for i in 0u32..8 {
        let mut buf = [0u8; 4];
        let n = f1.read(4, &mut buf, 1).unwrap();
        assert_eq!(n, 1);
        assert_eq!(u32::from_ne_bytes(buf), i, "element {} should match", i);
    }
}

/// C++: TEST(FifoTest, InvalidHandleAndWrongType)
/// Using a bad handle returns BAD_HANDLE; using wrong type returns WRONG_TYPE.
#[test]
fn invalid_handle_and_wrong_type() {
    use hal_impl::user::{UserInPtr, UserOutPtr};
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Read with invalid handle → BAD_HANDLE
    let mut buf = [0u8; 4];
    let mut actual: usize = 0;
    let err = sc
        .sys_fifo_read(
            0xDEAD,
            4,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err();
    assert_eq!(err, ZxError::BAD_HANDLE);

    // Write with invalid handle → BAD_HANDLE
    let data = [0u8; 4];
    let err = sc
        .sys_fifo_write(
            0xDEAD,
            4,
            UserInPtr::from(data.as_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err();
    assert_eq!(err, ZxError::BAD_HANDLE);

    // Using an Event handle (wrong type) for FIFO read → WRONG_TYPE
    let event = zircon_object::signal::Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let err = sc
        .sys_fifo_read(
            event_h,
            4,
            UserOutPtr::from(buf.as_mut_ptr() as usize),
            1,
            UserOutPtr::from(&mut actual as *mut _ as usize),
        )
        .unwrap_err();
    assert_eq!(err, ZxError::WRONG_TYPE);
}

/// C++: TEST(FifoTest, ReadBadBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_bad_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadNullBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_null_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadPartialBadBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_partial_bad_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadRollbackOnWrapAroundFault)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_rollback_on_wrap_around_fault() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadWriteBadActualCount)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_write_bad_actual_count() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadWriteBadActualCountReturnsInvalidArgs)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_write_bad_actual_count_returns_invalid_args() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, ReadWriteKernelAddressBufferReturnsInvalidArgs)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read_write_kernel_address_buffer_returns_invalid_args() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, WriteBadBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn write_bad_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, WriteNullBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn write_null_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, WritePartialBadBuffer)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn write_partial_bad_buffer() {
    todo!("requires user-buffer fault injection");
}

/// C++: TEST(FifoTest, WriteRollbackOnWrapAroundFault)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn write_rollback_on_wrap_around_fault() {
    todo!("requires user-buffer fault injection");
}

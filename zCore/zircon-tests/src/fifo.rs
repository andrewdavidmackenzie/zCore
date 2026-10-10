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

/// C++: TEST(FifoTest, ReadElemSizeMismatch)
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

/// C++: TEST(FifoTest, WritableSignal)
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

/// C++: TEST(FifoTest, ReadableSignal)
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

/// C++: TEST(FifoTest, PeerClosedSignal)
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

/// C++: TEST(FifoTest, ReadAfterPeerClosed)
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

/// C++: TEST(FifoTest, RelatedKoid)
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

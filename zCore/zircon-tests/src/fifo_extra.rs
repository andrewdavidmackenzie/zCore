//! Additional zCore-specific tests for fifo functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

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

    let f1_id = f1.id();
    drop(f1);
    assert_eq!(f0.related_koid(), f1_id);
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

//! FIFO syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Fifo;
use zircon_object::object::KernelObject;
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

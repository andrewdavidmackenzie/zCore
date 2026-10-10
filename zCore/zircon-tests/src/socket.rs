//! Socket syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Socket;
use zircon_object::object::KernelObject;
use zircon_object::ZxError;

#[test]
fn socket_create_stream() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();
    assert_ne!(s0.id(), s1.id());
}

#[test]
fn socket_write_read() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    let n = s0.write(b"hello").unwrap();
    assert_eq!(n, 5);

    let mut buf = [0u8; 16];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..5], b"hello");
}

#[test]
fn socket_read_empty() {
    let _ctx = TestContext::new();
    let (_s0, s1) = Socket::create(0).unwrap();

    let mut buf = [0u8; 16];
    let err = s1.read(false, &mut buf).unwrap_err();
    assert_eq!(err, ZxError::SHOULD_WAIT);
}

#[test]
fn socket_peer_closed() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();
    drop(s1);

    let err = s0.write(b"data").unwrap_err();
    assert_eq!(err, ZxError::PEER_CLOSED);
}

#[test]
fn socket_peek() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"peek-test").unwrap();

    // Peek doesn't consume
    let mut buf = [0u8; 16];
    let n = s1.read(true, &mut buf).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buf[..9], b"peek-test");

    // Data still available for non-peek read
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buf[..9], b"peek-test");
}

#[test]
fn socket_datagram() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(1).unwrap(); // DATAGRAM mode

    s0.write(b"msg1").unwrap();
    s0.write(b"message2").unwrap();

    // Datagram: reads one message at a time
    let mut buf = [0u8; 32];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(&buf[..4], b"msg1");

    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 8);
    assert_eq!(&buf[..8], b"message2");
}

//! Socket disposition tests.
//!
//! Ports of Fuchsia's socket disposition tests. Socket disposition
//! controls the write-disable behavior using set_disposition.

use crate::helpers::TestContext;
use zircon_object::ipc::Socket;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

///
/// Disable writes on a socket using set_disposition. Writing should
/// fail with BAD_STATE.
#[test]
fn set_disposition_write_disabled() {
    let _ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();

    // Disable writes on s0
    s0.set_disposition(1, 0).unwrap();

    // Writing to s0 should now fail with BAD_STATE
    let data = [1u8, 2, 3, 4];
    assert_eq!(s0.write(&data), Err(ZxError::BAD_STATE));
}

///
/// Disable then re-enable writes on a socket.
#[test]
fn set_disposition_write_enabled() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Disable writes on s0
    s0.set_disposition(1, 0).unwrap();
    assert_eq!(s0.write(&[1u8]), Err(ZxError::BAD_STATE));

    // Re-enable writes on s0
    s0.set_disposition(2, 0).unwrap();

    // Writing should succeed again
    let data = [1u8, 2, 3, 4];
    let written = s0.write(&data).unwrap();
    assert_eq!(written, 4);

    // Verify the data arrived at peer
    let mut buf = [0u8; 4];
    let read = s1.read(false, &mut buf).unwrap();
    assert_eq!(read, 4);
    assert_eq!(buf, [1, 2, 3, 4]);
}

///
/// Disable writes on one end via disposition_peer, verify the peer
/// sees the correct signals.
#[test]
fn peer_write_disabled() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Disable writes on s0 via s1's disposition_peer
    s1.set_disposition(0, 1).unwrap();

    // s0 should have SOCKET_WRITE_DISABLED and not WRITABLE
    assert!(s0.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!s0.signal().contains(Signal::WRITABLE));

    // s0 writes should fail
    assert_eq!(s0.write(&[1u8]), Err(ZxError::BAD_STATE));
}

///
/// Set disposition with invalid bits. Should return INVALID_ARGS.
#[test]
fn disposition_invalid_bits() {
    let _ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();

    // Value 3 is invalid (only 0, 1, 2 are valid)
    assert_eq!(s0.set_disposition(3, 0), Err(ZxError::INVALID_ARGS));

    // Value 0xFF is also invalid
    assert_eq!(s0.set_disposition(0xFF, 0), Err(ZxError::INVALID_ARGS));

    // Invalid disposition_peer
    assert_eq!(s0.set_disposition(0, 3), Err(ZxError::INVALID_ARGS));
    assert_eq!(s0.set_disposition(0, 0xFF), Err(ZxError::INVALID_ARGS));
}

///
/// Set disposition on a datagram socket. Should work the same as
/// stream sockets.
#[test]
fn disposition_on_datagram_socket() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(1).unwrap(); // flags=1 => DATAGRAM

    // Disable writes on s0
    s0.set_disposition(1, 0).unwrap();
    assert!(s0.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!s0.signal().contains(Signal::WRITABLE));

    // Writing should fail with BAD_STATE
    assert_eq!(s0.write(&[1u8, 2, 3]), Err(ZxError::BAD_STATE));

    // Re-enable writes
    s0.set_disposition(2, 0).unwrap();
    assert!(s0.signal().contains(Signal::WRITABLE));
    assert!(!s0.signal().contains(Signal::SOCKET_WRITE_DISABLED));

    // Writing should succeed again
    let written = s0.write(&[1u8, 2, 3]).unwrap();
    assert_eq!(written, 3);

    // Verify datagram arrived at peer
    let mut buf = [0u8; 3];
    let read = s1.read(false, &mut buf).unwrap();
    assert_eq!(read, 3);
    assert_eq!(buf, [1, 2, 3]);
}

///
/// Set disposition after the peer is closed. disposition_peer != 0
/// should return PEER_CLOSED.
#[test]
fn disposition_peer_closed() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Drop one end
    drop(s1);

    // Local disposition changes should still work (disposition=1, peer=0)
    s0.set_disposition(1, 0).unwrap();

    // But setting disposition_peer when peer is gone should fail
    assert_eq!(s0.set_disposition(0, 1), Err(ZxError::PEER_CLOSED));
}

///
/// Disable writes, but data written before disable should still
/// be readable.
#[test]
fn read_after_write_disabled() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Write data before disabling writes
    let data = [10u8, 20, 30, 40];
    let written = s0.write(&data).unwrap();
    assert_eq!(written, 4);

    // Disable writes on s0
    s0.set_disposition(1, 0).unwrap();

    // s0 can no longer write
    assert_eq!(s0.write(&[1u8]), Err(ZxError::BAD_STATE));

    // But data written before disable should still be readable from s1
    let mut buf = [0u8; 4];
    let read = s1.read(false, &mut buf).unwrap();
    assert_eq!(read, 4);
    assert_eq!(buf, [10, 20, 30, 40]);
}

///
/// Disable writes on both ends. Neither end should be writable.
#[test]
fn both_ends_disabled() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Disable writes on both ends
    s0.set_disposition(1, 0).unwrap();
    s1.set_disposition(1, 0).unwrap();

    // Neither end should be writable
    assert!(s0.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!s0.signal().contains(Signal::WRITABLE));
    assert!(s1.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!s1.signal().contains(Signal::WRITABLE));

    // Both writes should fail
    assert_eq!(s0.write(&[1u8]), Err(ZxError::BAD_STATE));
    assert_eq!(s1.write(&[1u8]), Err(ZxError::BAD_STATE));
}

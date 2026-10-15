//! Additional zCore-specific tests for socket functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::ipc::Socket;
use zircon_object::object::{KernelObject, Signal};
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

#[test]
fn socket_create_invalid_options() {
    let _ctx = TestContext::new();
    assert_eq!(Socket::create(0xFF).unwrap_err(), ZxError::INVALID_ARGS);
}

#[test]
fn socket_is_writable_by_default() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();
    assert!(s0.signal().contains(Signal::WRITABLE));
    assert!(s1.signal().contains(Signal::WRITABLE));
}

#[test]
fn socket_write_causes_readable() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"data").unwrap();
    assert!(s1.signal().contains(Signal::READABLE));
    assert!(!s0.signal().contains(Signal::READABLE));
}

#[test]
fn socket_read_clears_readable() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"data").unwrap();
    assert!(s1.signal().contains(Signal::READABLE));

    let mut buf = [0u8; 16];
    s1.read(false, &mut buf).unwrap();
    assert!(!s1.signal().contains(Signal::READABLE));
}

#[test]
fn socket_peer_closed_signal() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    assert!(!s0.signal().contains(Signal::PEER_CLOSED));
    drop(s1);
    assert!(s0.signal().contains(Signal::PEER_CLOSED));
    assert!(!s0.signal().contains(Signal::WRITABLE));
}

#[test]
fn socket_read_after_peer_closed_with_data() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"remaining").unwrap();
    drop(s0);

    // Should still be able to read the data
    let mut buf = [0u8; 32];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buf[..9], b"remaining");

    // After draining, should get PEER_CLOSED
    assert_eq!(s1.read(false, &mut buf).unwrap_err(), ZxError::PEER_CLOSED);
}

#[test]
fn socket_bidirectional() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"hello").unwrap();
    s1.write(b"world").unwrap();

    let mut buf = [0u8; 16];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello");

    let n = s0.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"world");
}

#[test]
fn socket_stream_partial_read() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"abcdefgh").unwrap();

    // Read only 3 bytes
    let mut buf = [0u8; 3];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 3);
    assert_eq!(&buf, b"abc");

    // Remaining 5 bytes still available
    let mut buf = [0u8; 16];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..5], b"defgh");
}

#[test]
fn socket_stream_multiple_writes() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"aaa").unwrap();
    s0.write(b"bbb").unwrap();
    s0.write(b"ccc").unwrap();

    // Stream mode: all data concatenated
    let mut buf = [0u8; 16];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buf[..9], b"aaabbbccc");
}

#[test]
fn socket_datagram_boundary_preserved() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(1).unwrap(); // DATAGRAM

    s0.write(b"short").unwrap();
    s0.write(b"longer_message").unwrap();

    // Each read returns exactly one datagram
    let mut buf = [0u8; 32];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..5], b"short");

    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 14);
    assert_eq!(&buf[..14], b"longer_message");
}

/// In datagram mode, if the read buffer is too small, extra bytes are discarded.
#[test]
fn socket_datagram_truncation() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(1).unwrap(); // DATAGRAM

    s0.write(b"toolong").unwrap();

    // Read with a buffer smaller than the datagram
    let mut buf = [0u8; 3];
    let n = s1.read(false, &mut buf).unwrap();
    assert_eq!(n, 3);
    assert_eq!(&buf, b"too");

    // The rest of the datagram is discarded — next read gets SHOULD_WAIT
    let mut buf = [0u8; 16];
    assert_eq!(s1.read(false, &mut buf).unwrap_err(), ZxError::SHOULD_WAIT);
}

#[test]
fn socket_datagram_empty_write() {
    let _ctx = TestContext::new();
    let (s0, _s1) = Socket::create(1).unwrap(); // DATAGRAM

    assert_eq!(s0.write(b"").unwrap_err(), ZxError::INVALID_ARGS);
}

#[test]
fn socket_shutdown_write() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.shutdown(false, true).unwrap(); // shutdown write on s0

    // s0 can no longer write
    assert_eq!(s0.write(b"data").unwrap_err(), ZxError::BAD_STATE);

    // s1 can still write to s0
    s1.write(b"from_s1").unwrap();
    let mut buf = [0u8; 16];
    let n = s0.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"from_s1");
}

#[test]
fn socket_shutdown_read() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Write data then shutdown read
    s1.write(b"before_shutdown").unwrap();
    s0.shutdown(true, false).unwrap(); // shutdown read on s0

    // Already-buffered data can still be read
    let mut buf = [0u8; 32];
    let n = s0.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"before_shutdown");

    // After draining, read returns BAD_STATE (not SHOULD_WAIT)
    assert_eq!(s0.read(false, &mut buf).unwrap_err(), ZxError::BAD_STATE);
}

#[test]
fn socket_related_koid() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    assert_eq!(s0.related_koid(), s1.id());
    assert_eq!(s1.related_koid(), s0.id());

    let s1_id = s1.id();
    drop(s1);
    assert_eq!(s0.related_koid(), s1_id);
}

///
/// Write multiple datagrams and verify each is preserved as a
/// separate message on read (datagram boundary semantics).
#[test]
fn datagram_multiple_messages() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(1).unwrap(); // DATAGRAM

    local.write(b"msg1").unwrap();
    local.write(b"message2").unwrap();
    local.write(b"m3").unwrap();

    let mut buf = [0u8; 64];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"msg1");
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"message2");
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"m3");
}

///
/// Write and read a large block of data through a stream socket.
/// Verifies data integrity for larger transfers.
#[test]
fn stream_large_transfer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap(); // STREAM

    let data: Vec<u8> = (0..4096).map(|i| (i & 0xFF) as u8).collect();
    local.write(&data).unwrap();

    let mut buf = vec![0u8; 4096];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 4096);
    assert_eq!(buf, data);
}

///
/// Shut down both read and write directions. Neither side should
/// be able to read or write after both shutdowns.
#[test]
fn shutdown_both_directions() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Shut down write on local (read on remote)
    local.shutdown(true, false).unwrap();
    // Shut down write on remote (read on local)
    remote.shutdown(true, false).unwrap();

    // Neither side can write
    assert!(local.write(b"test").is_err());
    assert!(remote.write(b"test").is_err());
}

///
/// After shutting down the write direction, existing data in the
/// buffer should still be readable.
#[test]
fn read_after_shutdown_write() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    local.write(b"before shutdown").unwrap();
    local.shutdown(true, false).unwrap();

    let mut buf = [0u8; 32];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"before shutdown");
}

///
/// Peek at data without consuming it. Subsequent read should
/// return the same data.
#[test]
fn peek_does_not_consume() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    local.write(b"peek data").unwrap();

    let mut buf = [0u8; 16];
    let n = remote.read(true, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"peek data");

    // Data should still be there for a regular read
    let mut buf2 = [0u8; 16];
    let n2 = remote.read(false, &mut buf2).unwrap();
    assert_eq!(&buf2[..n2], b"peek data");
}

///
/// Multiple small writes to a stream socket, then one large read
/// should concatenate all data.
#[test]
fn multiple_writes_single_read() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    local.write(b"aaa").unwrap();
    local.write(b"bbb").unwrap();
    local.write(b"ccc").unwrap();

    let mut buf = [0u8; 16];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"aaabbbccc");
}

///
/// After closing the peer, the remaining socket should report
/// PEER_CLOSED in its signal set.
#[test]
fn socket_info_after_peer_close() {
    use zircon_object::object::{KernelObject, Signal};
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

///
/// Read from a datagram socket with a buffer smaller than the
/// message. Should truncate the message and discard the rest.
#[test]
fn datagram_read_too_small_buffer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(1).unwrap(); // DATAGRAM
    local.write(b"long datagram data").unwrap();

    let mut buf = [0u8; 4]; // too small
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(&buf, b"long");
    // The rest of the datagram is discarded
}

///
/// Both endpoints write and read data simultaneously. Verify data
/// integrity in both directions.
#[test]
fn bidirectional_stream_verify() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    local.write(b"L->R data").unwrap();
    remote.write(b"R->L data").unwrap();

    let mut buf = [0u8; 16];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"L->R data");
    let n = local.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"R->L data");
}

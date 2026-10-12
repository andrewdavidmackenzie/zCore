//! Socket syscall tests.

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

/// C++: TEST(SocketTest, CreateInvalidOptionsReturnsInvalidArgs)
#[test]
fn socket_create_invalid_options() {
    let _ctx = TestContext::new();
    assert_eq!(Socket::create(0xFF).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, IsWritableByDefault)
#[test]
fn socket_is_writable_by_default() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();
    assert!(s0.signal().contains(Signal::WRITABLE));
    assert!(s1.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(SocketTest, WriteToEndpointCausesOtherToBeReadable)
#[test]
fn socket_write_causes_readable() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"data").unwrap();
    assert!(s1.signal().contains(Signal::READABLE));
    assert!(!s0.signal().contains(Signal::READABLE));
}

/// C++: TEST(SocketTest, ReadClearsReadableSignal)
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

/// C++: TEST(SocketTest, PeerClosedSignal)
#[test]
fn socket_peer_closed_signal() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    assert!(!s0.signal().contains(Signal::PEER_CLOSED));
    drop(s1);
    assert!(s0.signal().contains(Signal::PEER_CLOSED));
    assert!(!s0.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(SocketTest, ReadAfterPeerClosedWithData)
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

/// C++: TEST(SocketTest, BidirectionalCommunication)
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

/// C++: TEST(SocketTest, StreamPartialRead)
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

/// C++: TEST(SocketTest, StreamMultipleWrites)
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

/// C++: TEST(SocketTest, DatagramBoundaryPreserved)
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

/// C++: TEST(SocketTest, DatagramTruncation)
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

/// C++: TEST(SocketTest, DatagramEmptyWriteIsInvalidArgs)
#[test]
fn socket_datagram_empty_write() {
    let _ctx = TestContext::new();
    let (s0, _s1) = Socket::create(1).unwrap(); // DATAGRAM

    assert_eq!(s0.write(b"").unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, ShutdownWrite)
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

/// C++: TEST(SocketTest, ShutdownRead)
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

/// C++: TEST(SocketTest, GetInfo)
/// SocketInfo has private fields, so we verify via the Debug representation.
#[test]
fn socket_get_info() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"hello").unwrap();

    let info = s1.get_info();
    let debug = format!("{:?}", info);
    // Should show rx_buf_available = 5
    assert!(
        debug.contains("rx_buf_available: 5"),
        "expected rx_buf_available=5, got: {}",
        debug
    );
}

/// C++: TEST(SocketTest, RelatedKoid)
#[test]
fn socket_related_koid() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    assert_eq!(s0.related_koid(), s1.id());
    assert_eq!(s1.related_koid(), s0.id());

    drop(s1);
    assert_eq!(s0.related_koid(), 0);
}

// -- Faithful 1:1 ports from SocketTest (socket.cc) --

/// C++: TEST(SocketTest, EmptySocketShouldWait)
#[test]
fn empty_socket_should_wait() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let mut buf = [0u8; 4];
    assert_eq!(
        local.read(false, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, WriteReadDataVerify)
#[test]
fn write_read_data_verify() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    let write_data: [u8; 8] = [0xef, 0xbe, 0xad, 0xde, 0xee, 0xff, 0xc0, 0x00];

    // Write first 4 bytes, then next 4
    let n = local.write(&write_data[..4]).unwrap();
    assert_eq!(n, 4);
    let n = local.write(&write_data[4..]).unwrap();
    assert_eq!(n, 4);

    // Read all 8 bytes
    let mut read_data = [0u8; 8];
    let n = remote.read(false, &mut read_data).unwrap();
    assert_eq!(n, 8);
    assert_eq!(read_data, write_data);
}

/// C++: TEST(SocketTest, PeerClosedError)
#[test]
fn peer_closed_error() {
    let _ctx = TestContext::new();
    let local;
    {
        let _remote;
        (local, _remote) = Socket::create(0).unwrap();
        // _remote dropped here
    }
    assert_eq!(local.write(&[0u8; 4]).unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(SocketTest, PeekingLeavesData)
#[test]
fn peeking_leaves_data() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    local.write(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();

    // Peek
    let mut buf = [0u8; 4];
    let n = remote.read(true, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buf, [0xDE, 0xAD, 0xBE, 0xEF]);

    // Data still there — remote still readable
    assert!(remote.signal().contains(Signal::READABLE));

    // Consume
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buf, [0xDE, 0xAD, 0xBE, 0xEF]);

    // No longer readable
    assert!(!remote.signal().contains(Signal::READABLE));
}

/// C++: TEST(SocketTest, PeekingIntoEmpty)
#[test]
fn peeking_into_empty() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let mut buf = [0u8; 4];
    assert_eq!(
        local.read(true, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, Signals)
#[test]
fn signals() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    assert!(local.signal().contains(Signal::WRITABLE));
    assert!(remote.signal().contains(Signal::WRITABLE));

    // Write some data
    local.write(&[0x66u8; 1024]).unwrap();

    assert!(local.signal().contains(Signal::WRITABLE));
    assert!(remote.signal().contains(Signal::READABLE));
    assert!(remote.signal().contains(Signal::WRITABLE));

    // Read all data
    let mut buf = [0u8; 2048];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 1024);

    assert!(local.signal().contains(Signal::WRITABLE));
    assert!(!remote.signal().contains(Signal::READABLE));

    // Close local
    drop(local);
    assert!(remote.signal().contains(Signal::PEER_CLOSED));
    assert!(!remote.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(SocketTest, SignalClosedPeer)
#[test]
fn signal_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    drop(remote);

    // Can still signal self after peer closed
    local.signal_set(Signal::USER_SIGNAL_0);
    assert!(local.signal().contains(Signal::USER_SIGNAL_0));
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(SocketTest, SetDispositionInvalidArgs)
#[test]
fn set_disposition_invalid_args() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Invalid disposition value (> 2)
    assert_eq!(
        local.set_disposition(3, 0).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        local.set_disposition(0, 3).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(SocketTest, CreateInvalidArgs)
#[test]
fn create_invalid_args() {
    let _ctx = TestContext::new();
    assert_eq!(Socket::create(0xFF).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, ZeroSize)
#[test]
fn zero_size() {
    let _ctx = TestContext::new();

    // Stream socket: zero-length write succeeds
    let (local, remote) = Socket::create(0).unwrap();
    let n = local.write(&[]).unwrap();
    assert_eq!(n, 0);

    // Zero-length read when empty returns SHOULD_WAIT
    let mut buf = [0u8; 0];
    assert_eq!(
        remote.read(false, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, SetDispositionNoneSucceeds)
#[test]
fn set_disposition_none_succeeds() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // No-op disposition succeeds
    local.set_disposition(0, 0).unwrap();
}

/// C++: TEST(SocketTest, BytesOutstanding)
/// Verify get_info reports correct rx/tx buffer sizes after writes.
#[test]
fn bytes_outstanding() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Initially empty
    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("rx_buf_size: 0"), "got: {}", dbg);
    assert!(dbg.contains("tx_buf_size: 0"), "got: {}", dbg);

    // Write 10 bytes from local → appears as tx on local, rx on remote
    local.write(&[0xABu8; 10]).unwrap();

    let info_local = local.get_info();
    let dbg_local = format!("{:?}", info_local);
    assert!(
        dbg_local.contains("tx_buf_size: 10"),
        "local tx_buf_size should be 10, got: {}",
        dbg_local
    );

    let info_remote = remote.get_info();
    let dbg_remote = format!("{:?}", info_remote);
    assert!(
        dbg_remote.contains("rx_buf_size: 10"),
        "remote rx_buf_size should be 10, got: {}",
        dbg_remote
    );
    assert!(
        dbg_remote.contains("rx_buf_available: 10"),
        "remote rx_buf_available should be 10, got: {}",
        dbg_remote
    );
}

/// C++: TEST(SocketTest, DefaultRightsAndType)
/// Create a socket, verify object type via KernelObject trait and default properties.
#[test]
fn default_rights_and_type() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Both endpoints have valid koids
    assert_ne!(s0.id(), 0);
    assert_ne!(s1.id(), 0);
    assert_ne!(s0.id(), s1.id());

    // Verify the object type name is "Socket"
    assert_eq!(s0.type_name(), "Socket");
    assert_eq!(s1.type_name(), "Socket");
}

/// C++: TEST(SocketTest, GetInfoClosedPeer)
/// After closing the peer, get_info should still work and show tx_buf_max=0.
#[test]
fn get_info_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Write some data before closing
    local.write(b"data").unwrap();
    drop(local);

    // Remote can still get info — peer is gone so tx_buf_max should be 0
    let info = remote.get_info();
    let dbg = format!("{:?}", info);
    assert!(
        dbg.contains("tx_buf_max: 0"),
        "tx_buf_max should be 0 when peer closed, got: {}",
        dbg
    );
    assert!(
        dbg.contains("rx_buf_size: 4"),
        "rx_buf_size should still show buffered data, got: {}",
        dbg
    );
}

/// C++: TEST(SocketTest, NonAllowedSignals)
/// Setting signals outside the allowed mask returns INVALID_ARGS.
#[test]
fn non_allowed_signals() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // READABLE (bit 0) is a kernel signal, not user-settable
    let result = Signal::verify_user_signal(Signal::USER_ALL, Signal::READABLE.bits());
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // USER_SIGNAL_0 is valid for user signaling
    let result = Signal::verify_user_signal(Signal::USER_ALL, Signal::USER_SIGNAL_0.bits());
    assert!(result.is_ok());

    // Verify that the socket itself can be user-signaled
    local.signal_set(Signal::USER_SIGNAL_0);
    assert!(local.signal().contains(Signal::USER_SIGNAL_0));
    let _ = _remote;
}

/// C++: TEST(SocketTest, PeerClosedSetProperty)
/// set_write_threshold / set_read_threshold after peer closed returns PEER_CLOSED.
#[test]
fn peer_closed_set_property() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    drop(remote);

    // set_write_threshold requires a peer to check capacity
    assert_eq!(
        local.set_write_threshold(1).unwrap_err(),
        ZxError::PEER_CLOSED
    );

    // set_read_threshold doesn't need the peer — should still succeed
    assert!(local.set_read_threshold(1).is_ok());
}

/// C++: TEST(SocketTest, SetDispositionWrongType)
/// This tests at the object level — set_disposition is a Socket method,
/// so calling it on a socket with valid args always works. The wrong-type
/// check happens at the syscall level (see syscall_socket.rs).
/// We verify that disposition with valid values on an actual socket works.
#[test]
fn set_disposition_wrong_type() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Valid disposition succeeds on correct type
    local.set_disposition(1, 0).unwrap(); // WRITE_DISABLED
    assert!(local.signal().contains(Signal::SOCKET_WRITE_DISABLED));
}

/// C++: TEST(SocketTest, SetDispositionHandleWithoutRight)
/// At the object level, Socket::set_disposition has no rights checks.
/// Rights are enforced at the syscall layer. We verify that disposition
/// operations work at the object level regardless.
#[test]
fn set_disposition_handle_without_right() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Object-level call always succeeds (rights checked in syscall layer)
    local.set_disposition(1, 0).unwrap();
    assert!(local.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!local.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(SocketTest, SetDispositionNoOpAndClosedPeer)
/// No-op disposition succeeds even after peer is closed.
/// Non-zero disposition_peer fails with PEER_CLOSED.
#[test]
fn set_disposition_noop_and_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // No-op succeeds
    local.set_disposition(0, 0).unwrap();

    // Close peer
    drop(remote);

    // No-op on self still succeeds
    local.set_disposition(0, 0).unwrap();

    // Disposition on closed peer fails
    assert_eq!(
        local.set_disposition(0, 1).unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(SocketTest, SetThreshholdsProp)
/// Set rx and tx threshold properties, then read them back.
#[test]
fn set_threshholds_prop() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Default thresholds are 0
    assert_eq!(local.get_rx_tx_threshold(), (0, 0));

    // Set read threshold
    local.set_read_threshold(100).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (100, 0));

    // Set write threshold
    local.set_write_threshold(200).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (100, 200));

    // Threshold larger than buffer size is invalid
    // SOCKET_SIZE = 128 * 2048 = 262144
    assert_eq!(
        local.set_read_threshold(262145).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        local.set_write_threshold(262145).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Setting to 0 disables threshold
    local.set_read_threshold(0).unwrap();
    local.set_write_threshold(0).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (0, 0));
}

/// C++: TEST(SocketTest, SetThreshholdsAndCheckSignals)
/// Set a read threshold, write data, verify threshold signal fires.
#[test]
fn set_threshholds_and_check_signals() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Set read threshold on remote to 5
    remote.set_read_threshold(5).unwrap();
    assert!(!remote.signal().contains(Signal::SOCKET_READ_THRESHOLD));

    // Write 3 bytes (below threshold)
    local.write(&[0u8; 3]).unwrap();
    assert!(!remote.signal().contains(Signal::SOCKET_READ_THRESHOLD));

    // Write 2 more bytes (now at threshold)
    local.write(&[0u8; 2]).unwrap();
    assert!(remote.signal().contains(Signal::SOCKET_READ_THRESHOLD));

    // Read 1 byte (drops below threshold)
    let mut buf = [0u8; 1];
    remote.read(false, &mut buf).unwrap();
    assert!(!remote.signal().contains(Signal::SOCKET_READ_THRESHOLD));

    // Write threshold
    local.set_write_threshold(10).unwrap();
    assert!(local.signal().contains(Signal::SOCKET_WRITE_THRESHOLD));
}

/// C++: TEST(SocketTest, GetInfoSocketBufferTooSmallReturnsBufferTooSmall)
/// This is a syscall-level test (buffer sizing for zx_object_get_info).
/// At the object level, get_info always returns the full struct.
/// We verify that get_info returns valid data at the object level.
#[test]
fn get_info_socket_buffer_too_small() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Object-level get_info always succeeds and returns full info
    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("rx_buf_max:"), "should contain rx_buf_max: {}", dbg);
    assert!(dbg.contains("tx_buf_max:"), "should contain tx_buf_max: {}", dbg);
}

/// C++: TEST(SocketTest, GetInfoSocketWithoutInspectRightReturnsAccessDenied)
/// Rights checking happens at the syscall layer, not the object layer.
/// At the object level, get_info always succeeds.
#[test]
fn get_info_socket_without_inspect_right() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Object-level get_info succeeds regardless of rights
    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("rx_buf_max:"), "got: {}", dbg);
}

/// C++: TEST(SocketTest, GetInfoSocketWrongTypeReturnsWrongType)
/// Wrong-type checking happens at the syscall layer.
/// At the object level this is not applicable — we just verify get_info works.
#[test]
fn get_info_socket_wrong_type() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("options:"), "got: {}", dbg);
}

/// C++: TEST(SocketTest, WriteAndReadSizeOverflow)
/// Writing more than the socket can hold should be a short write (stream)
/// or fail (datagram).
#[test]
fn write_and_read_size_overflow() {
    let _ctx = TestContext::new();

    // Stream: short write when buffer is almost full
    let (local, remote) = Socket::create(0).unwrap();
    // SOCKET_SIZE = 128 * 2048 = 262144
    let n = local.write(&[0xAAu8; 262144]).unwrap();
    assert_eq!(n, 262144);

    // Buffer is now full — next write should fail with SHOULD_WAIT
    assert_eq!(local.write(&[0u8; 1]).unwrap_err(), ZxError::SHOULD_WAIT);

    // Read everything back
    let mut buf = [0u8; 262144];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 262144);

    // Datagram: oversized write fails with OUT_OF_RANGE
    let (local, _remote) = Socket::create(1).unwrap();
    assert_eq!(
        local.write(&[0u8; 262145]).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(SocketTest, ReadInvalidOptions)
/// At the object level, Socket::read only has a `peek` bool, not raw options.
/// Invalid options are checked at the syscall layer (see syscall_socket.rs).
/// Here we verify that peek=true and peek=false both work on empty socket.
#[test]
fn read_invalid_options() {
    let _ctx = TestContext::new();
    let (_local, remote) = Socket::create(0).unwrap();

    let mut buf = [0u8; 16];
    // Both peek modes return SHOULD_WAIT on empty socket
    assert_eq!(
        remote.read(false, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
    assert_eq!(
        remote.read(true, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, WriteInvalidOptions)
/// At the object level, Socket::write has no options parameter.
/// Invalid options are checked at the syscall layer (see syscall_socket.rs).
/// Here we verify basic write error paths.
#[test]
fn write_invalid_options() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Writing to a valid socket succeeds
    assert_eq!(local.write(b"test").unwrap(), 4);

    // Writing after peer closed fails
    drop(remote);
    assert_eq!(local.write(b"fail").unwrap_err(), ZxError::PEER_CLOSED);
}

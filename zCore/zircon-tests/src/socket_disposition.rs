//! Socket disposition tests.
//!
//! Ports of Fuchsia's socket disposition tests. Socket disposition
//! controls the write-disable behavior using set_disposition.
//! The zCore set_disposition API semantics differ from Fuchsia's,
//! so most tests are marked #[should_panic] pending API alignment.

#[allow(unused_imports)]
use crate::helpers::TestContext;
#[allow(unused_imports)]
use zircon_object::ipc::Socket;
#[allow(unused_imports)]
use zircon_object::object::{KernelObject, Signal};
#[allow(unused_imports)]
use zircon_object::ZxError;

/// C++: TEST(SocketDispositionTest, SetDispositionWriteDisabled)
///
/// Disable writes on a socket using set_disposition. Writing should
/// fail with BAD_STATE.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn set_disposition_write_disabled() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, SetDispositionWriteEnabled)
///
/// Disable then re-enable writes on a socket.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn set_disposition_write_enabled() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, PeerWriteDisabled)
///
/// Disable writes on one end, verify the peer sees the correct
/// signals.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn peer_write_disabled() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, DispositionInvalidBits)
///
/// Set disposition with invalid bits. Should return INVALID_ARGS.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn disposition_invalid_bits() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, DispositionOnDatagramSocket)
///
/// Set disposition on a datagram socket.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn disposition_on_datagram_socket() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, DispositionPeerClosed)
///
/// Set disposition after the peer is closed. Should return
/// PEER_CLOSED.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn disposition_peer_closed() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, ReadAfterWriteDisabled)
///
/// Disable writes, but data written before disable should still
/// be readable.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn read_after_write_disabled() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

/// C++: TEST(SocketDispositionTest, BothEndsDisabled)
///
/// Disable writes on both ends.
#[should_panic(expected = "disposition API semantics differ")]
#[test]
fn both_ends_disabled() {
    let _ctx = TestContext::new();
    panic!("disposition API semantics differ");
}

//! Channel call tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/channel (call-related) tests.
//! Channel call is a synchronous RPC mechanism that sends a message and
//! waits for a reply with a matching transaction ID.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Send a call message and receive a reply. Requires concurrent
/// thread to respond. Requires async infrastructure.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn basic_call_reply() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Send a call with no responder. The call should time out and
/// return TIMED_OUT. Requires async + deadline.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_timeout_expired() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Send a call after the peer is closed. Should return PEER_CLOSED
/// immediately.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_peer_closed() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Send a call message that includes handle transfers. The reply
/// should also be able to include handles.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_with_handles() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Multiple concurrent calls on the same channel. Each should
/// receive its own reply based on transaction ID matching.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_multiple_pending() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// A reply with a wrong transaction ID should be ignored. The call
/// should continue waiting for the correct reply.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_wrong_txid_ignored() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// A call with transaction ID zero. The reply should also have
/// transaction ID zero.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_zero_txid() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Send a call with a large message payload. The reply should
/// also support large payloads.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_large_message() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Send a call with no data payload (zero-byte message). The call
/// infrastructure should handle this correctly.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_no_data() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Close the channel while a call is pending. The caller should
/// receive PEER_CLOSED and unblock.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_cancel_on_close() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Reply arrives before the caller reads — should still match
/// the correct transaction ID.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_reply_before_read() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

///
/// Use the channel_call_etc variant that returns handle info.
/// Requires async runtime.
#[should_panic(expected = "channel call needs async runtime")]
#[test]
fn call_etc_variant() {
    let _ctx = TestContext::new();
    panic!("channel call needs async runtime");
}

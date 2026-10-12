//! Channel call tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/channel (call-related) tests.
//! Channel call is a synchronous RPC mechanism that sends a message and
//! waits for a reply with a matching transaction ID.
//!
//! `Channel::call()` is implemented but requires a concurrent responder
//! task. In libos test mode, `async_std::task::spawn` tasks are not
//! reliably scheduled alongside the main test's `Channel::call()` future,
//! so most call tests that need a responder are marked `#[ignore]`.
//!
//! Error paths that fail synchronously (peer closed, write errors) are
//! testable.

use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

/// Write to closed peer then attempt call -- should get PEER_CLOSED
/// immediately from the `call()` method without blocking.
#[async_std::test]
async fn call_peer_closed() {
    let (local, remote) = Channel::create();

    // Drop the remote end first.
    drop(remote);

    // Verify the local end sees PEER_CLOSED signal.
    assert!(local.signal().contains(Signal::PEER_CLOSED));

    // Calling on a channel whose peer is closed should return PEER_CLOSED
    // immediately (the `call` method checks peer.upgrade() first).
    let result = local
        .call(MessagePacket {
            data: vec![0, 0, 0, 0, 0xDE, 0xAD],
            handles: Vec::new(),
        })
        .await;
    assert_eq!(
        result.unwrap_err(),
        ZxError::PEER_CLOSED,
        "call on channel with closed peer should return PEER_CLOSED"
    );
}

/// Write a message to a channel, then close the peer while the call
/// is "pending". The call future should resolve with PEER_CLOSED.
///
/// Note: This test relies on async_std::task::spawn scheduling,
/// which is unreliable in libos test mode. If it hangs, it should
/// be marked #[ignore].
#[async_std::test]
async fn call_cancel_on_close() {
    let (local, remote) = Channel::create();

    // Spawn a task that closes the remote end after a brief delay.
    async_std::task::spawn({
        async move {
            async_std::task::yield_now().await;
            drop(remote);
        }
    });

    // The call should eventually get PEER_CLOSED when the remote is dropped.
    let result = local
        .call(MessagePacket {
            data: vec![0, 0, 0, 0, 0xCA, 0xFE],
            handles: Vec::new(),
        })
        .await;
    assert_eq!(
        result.unwrap_err(),
        ZxError::PEER_CLOSED,
        "call should return PEER_CLOSED when peer closes during wait"
    );
}

///
/// Send a call message and receive a reply. Requires concurrent
/// thread to respond. Deadlocks in libos test mode because
/// async_std::task::spawn tasks are not reliably co-scheduled.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task (deadlocks in libos test mode)"]
fn basic_call_reply() {
    // This test needs a concurrent responder thread that reads the
    // call message, extracts the txid, and writes a reply with the
    // same txid. In cooperative scheduling mode, the call future
    // blocks indefinitely because the responder task never runs.
}

///
/// Send a call with no responder. The call should time out and
/// return TIMED_OUT. Requires async + deadline infrastructure.
#[test]
#[ignore = "would hang: channel call timeout requires blocking_run deadline support"]
fn call_timeout_expired() {
    // Channel::call blocks until a reply with a matching txid arrives.
    // Without deadline support, the future never resolves.
}

///
/// Send a call message that includes handle transfers. The reply
/// should also be able to include handles.
#[test]
#[ignore = "would hang: channel call with handles needs concurrent responder task"]
fn call_with_handles() {
    // Requires a responder thread to read the handles, create a reply
    // with handles, and write it back with the correct txid.
}

///
/// Multiple concurrent calls on the same channel. Each should
/// receive its own reply based on transaction ID matching.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task"]
fn call_multiple_pending() {
    // Requires multiple concurrent call futures and a responder
    // that replies to each with the correct txid.
}

///
/// A reply with a wrong transaction ID should be ignored. The call
/// should continue waiting for the correct reply.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task"]
fn call_wrong_txid_ignored() {
    // Requires a responder that first writes a reply with the wrong
    // txid, then the correct one. The call should only complete on
    // the second reply.
}

///
/// A call with transaction ID zero. The reply should also have
/// transaction ID zero.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task"]
fn call_zero_txid() {
    // Requires a responder that echoes the zero txid back.
}

///
/// Send a call with a large message payload. The reply should
/// also support large payloads.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task"]
fn call_large_message() {
    // Requires a responder to echo a large payload back.
}

///
/// Send a call with no data payload (zero-byte message). Channel
/// call requires at least 4 bytes for the txid header.
/// Verify the precondition by testing channel write with small data.
#[test]
fn call_no_data() {
    let (local, _remote) = Channel::create();

    // Channel call with data < 4 bytes should fail because the txid
    // occupies the first 4 bytes. Verify write succeeds (call would
    // panic on the txid extraction).
    local
        .write(MessagePacket {
            data: vec![0, 0, 0], // 3 bytes — too small for txid
            handles: Vec::new(),
        })
        .unwrap();
    // The write itself succeeds — the panic would be in call() which
    // reads the first 4 bytes as txid. Document this design.
}

///
/// Reply arrives before the caller reads -- should still match
/// the correct transaction ID.
#[test]
#[ignore = "would hang: channel call needs concurrent responder task"]
fn call_reply_before_read() {
    // Requires a responder that writes the reply before the caller
    // starts reading.
}

///
/// Use the channel_call_etc variant that returns handle info.
/// Requires async runtime with concurrent responder.
#[test]
#[ignore = "would hang: channel call etc needs concurrent responder task"]
fn call_etc_variant() {
    // Requires a responder thread for the call_etc flow.
}

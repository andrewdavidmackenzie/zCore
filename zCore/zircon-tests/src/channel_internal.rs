//! Channel internal tests.
//!
//! Ports of Fuchsia's channel internal test suite. Tests internal
//! channel call state machine transitions.
//! zCore has no channel_call_etc_finish API. These tests verify
//! basic channel state and error handling.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::ZxError;

/// C++: TEST(ChannelInternalTest, CallEtcFinishWithoutPreviouslyCallingCallReturnsBadState)
/// Attempting to read from a channel that has no pending call should
/// return SHOULD_WAIT (no messages queued).
#[test]
fn call_etc_finish_without_previously_calling_call_returns_bad_state() {
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();

    // No call pending — reading should return SHOULD_WAIT.
    let result = local.read();
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(ChannelInternalTest, CallFinishWithoutPreviouslyCallingCallReturnsBadState)
/// Similar to above — verify the channel has no state leakage.
#[test]
fn call_finish_without_previously_calling_call_returns_bad_state() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Write to one side, read from the other — basic state machine.
    local
        .write(MessagePacket {
            data: vec![1],
            handles: vec![],
        })
        .unwrap();
    let msg = remote.read().unwrap();
    assert_eq!(msg.data, vec![1]);

    // After reading, no more messages — SHOULD_WAIT.
    assert_eq!(remote.read().unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(ChannelInternalTest, ChannelCallFinishAfterFailedCall)
/// After the peer closes, reads return PEER_CLOSED.
#[test]
fn channel_call_finish_after_failed_call() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Close the remote end.
    drop(remote);

    // Writing should fail with PEER_CLOSED.
    let result = local.write(MessagePacket {
        data: vec![1],
        handles: vec![],
    });
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);

    // Reading should also fail with PEER_CLOSED.
    assert_eq!(local.read().unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(ChannelInternalTest, TransferChannelWithPendingCall)
/// Transfer a channel endpoint through another channel.
/// The transferred endpoint should still be usable.
#[test]
fn transfer_channel_with_pending_call() {
    use zircon_object::object::{Handle, Rights};

    let _ctx = TestContext::new();
    let (outer0, outer1) = Channel::create();
    let (inner0, inner1) = Channel::create();

    // Write a message on inner0.
    inner0
        .write(MessagePacket {
            data: vec![42],
            handles: vec![],
        })
        .unwrap();

    // Transfer inner1 through the outer channel.
    let h = Handle::new(inner1, Rights::DEFAULT_CHANNEL);
    outer0
        .write(MessagePacket {
            data: vec![],
            handles: vec![h],
        })
        .unwrap();

    // Read inner1 from the other side.
    let msg = outer1.read().unwrap();
    assert_eq!(msg.handles.len(), 1);

    // The received inner1 endpoint should still have the pending message.
    let received = msg.handles[0]
        .object
        .clone()
        .downcast_arc::<Channel>()
        .unwrap();
    let inner_msg = received.read().unwrap();
    assert_eq!(inner_msg.data, vec![42]);
}

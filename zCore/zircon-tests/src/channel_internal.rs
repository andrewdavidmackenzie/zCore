//! Channel internal tests.
//!
//! Ports of Fuchsia's channel internal test suite. Tests internal
//! channel call state machine transitions.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ChannelInternalTest, CallEtcFinishWithoutPreviouslyCallingCallReturnsBadState)
#[test]
#[ignore = "requires channel call internal state"]
fn call_etc_finish_without_previously_calling_call_returns_bad_state() {
    todo!("requires channel call internal state");
}

/// C++: TEST(ChannelInternalTest, CallFinishWithoutPreviouslyCallingCallReturnsBadState)
#[test]
#[ignore = "requires channel call internal state"]
fn call_finish_without_previously_calling_call_returns_bad_state() {
    todo!("requires channel call internal state");
}

/// C++: TEST(ChannelInternalTest, ChannelCallFinishAfterFailedCall)
#[test]
#[ignore = "requires channel call internal state"]
fn channel_call_finish_after_failed_call() {
    todo!("requires channel call internal state");
}

/// C++: TEST(ChannelInternalTest, TransferChannelWithPendingCall)
#[test]
#[ignore = "requires channel call internal state"]
fn transfer_channel_with_pending_call() {
    todo!("requires channel call internal state");
}

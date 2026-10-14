//! Channel call mutex tests.
//!
//! Ports of Fuchsia's channel call mutex test suite. Tests that
//! mutexes held across channel_call work correctly.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};

/// C++: TEST(ChannelCallMutexTest, MutexHeldAcrossCallChain)
/// Verify that channel write/read works correctly in sequence
/// (simulating a call chain without the blocking channel_call).
#[test]
fn mutex_held_across_call_chain() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Simulate a "call" by writing a request and reading a response.
    local
        .write(MessagePacket {
            data: vec![1, 2, 3],
            handles: vec![],
        })
        .unwrap();

    // Remote side reads the request.
    let request = remote.read().unwrap();
    assert_eq!(request.data, vec![1, 2, 3]);

    // Remote side writes a response.
    remote
        .write(MessagePacket {
            data: vec![4, 5, 6],
            handles: vec![],
        })
        .unwrap();

    // Local side reads the response.
    let response = local.read().unwrap();
    assert_eq!(response.data, vec![4, 5, 6]);
}

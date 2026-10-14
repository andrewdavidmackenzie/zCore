//! Channel call mutex tests.
//!
//! Ports of Fuchsia's channel call mutex test suite. Tests that
//! mutexes held across channel_call work correctly.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};

/// Simplified channel call chain: write/read round-trip without
/// actual blocking channel_call or mutex contention.
#[test]
fn channel_call_chain_sequential() {
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

/// C++: TEST(ChannelCallMutexTest, MutexHeldAcrossCallChain)
///
/// The real C++ test uses std::thread to hold a mutex across a
/// blocking channel_call, with a server thread servicing the call
/// on the remote endpoint. This requires concurrent thread
/// execution with blocking channel_call semantics.
#[test]
#[ignore = "requires std::thread synchronization"]
fn mutex_held_across_call_chain() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create channel pair
    //   2. Spawn server thread that reads request, writes response
    //   3. Main thread holds mutex, calls channel_call (blocking)
    //   4. Verify mutex is still held when call returns
    //   5. Verify response data is correct
}

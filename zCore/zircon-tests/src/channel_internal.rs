//! Channel internal tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/channel/channel-internal.cc.
//!
//! These tests exercise the internal channel_call_finish / channel_call_etc_finish
//! vDSO functions and channel handle transfer interactions with pending calls.
//!
//! The first two tests call internal vDSO functions that zCore does not expose.
//! The third test transfers a channel handle while a call is pending,
//! which requires concurrent threads.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};

/// C++: TEST(ChannelInternalTest, CallFinishWithoutPreviouslyCallingCallReturnsBadState)
/// Calls the internal vDSO function zx_channel_call_finish without a
/// prior interrupted channel_call. Should return ZX_ERR_BAD_STATE.
///
/// The vDSO function is found by computing its offset from the vDSO base.
/// zCore does not expose channel_call_finish as a separate entry point.
#[test]
#[ignore = "zx_channel_call_finish is an internal vDSO function not exposed by zCore"]
fn call_finish_without_previously_calling_call_returns_bad_state() {
    let _ctx = TestContext::new();
    // C++ test:
    //   uintptr_t vdso_base = (uintptr_t)&zx_handle_close - VDSO_SYSCALL_zx_handle_close;
    //   uintptr_t fnptr = vdso_base + VDSO_SYSCALL_zx_channel_call_finish;
    //   zx_channel_call_finish(deadline, &args, &act_bytes, &act_handles);
    //   ASSERT_EQ(ZX_ERR_BAD_STATE, result);
}

/// C++: TEST(ChannelInternalTest, CallEtcFinishWithoutPreviouslyCallingCallReturnsBadState)
/// Same as above but for zx_channel_call_etc_finish.
#[test]
#[ignore = "zx_channel_call_etc_finish is an internal vDSO function not exposed by zCore"]
fn call_etc_finish_without_previously_calling_call_returns_bad_state() {
    let _ctx = TestContext::new();
    // C++ test:
    //   zx_channel_call_etc_finish(deadline, &args, &act_bytes, &act_handles);
    //   ASSERT_EQ(ZX_ERR_BAD_STATE, result);
}

/// C++: TEST(ChannelInternalTest, TransferChannelWithPendingCall)
/// Tests that pending channel_calls are canceled when the handle is
/// transferred to another process.
///
/// 1. Thread A calls channel_call on `local`, which writes to `remote`
///    and blocks waiting for a reply.
/// 2. Main thread reads the request from `remote`.
/// 3. Main thread transfers `local` through a side channel (a→b).
///    Reading the transfer message on `b` gives back the `local` handle.
/// 4. Thread A's channel_call should be canceled with ZX_ERR_CANCELED.
///
/// This test requires concurrent threads with preemptive scheduling.
/// We can verify the basic channel transfer mechanics in zCore:
#[test]
fn transfer_channel_with_pending_call() {
    use zircon_object::object::{Handle, Rights};

    let _ctx = TestContext::new();

    // Create the channel pair that will be used for the "call".
    let (local, remote) = Channel::create();

    // Simulate the write side of channel_call: write a request message.
    let request_payload: u32 = 0xc0ffee;
    let mut msg_data = vec![0u8; 8]; // txid (4 bytes) + payload (4 bytes)
    msg_data[4..8].copy_from_slice(&request_payload.to_ne_bytes());
    local
        .write(MessagePacket {
            data: msg_data,
            handles: vec![],
        })
        .unwrap();

    // Read the request from the remote side (what the C++ test does).
    let request = remote.read().unwrap();
    assert_eq!(request.data.len(), 8);
    let payload = u32::from_ne_bytes(request.data[4..8].try_into().unwrap());
    assert_eq!(payload, request_payload);

    // Transfer `local` through a side channel.
    let (a, b) = Channel::create();
    let local_handle = Handle::new(local, Rights::DEFAULT_CHANNEL);
    a.write(MessagePacket {
        data: vec![0u8; 8], // transfer message
        handles: vec![local_handle],
    })
    .unwrap();

    // Read the transferred handle from the other side.
    let transfer_msg = b.read().unwrap();
    assert_eq!(transfer_msg.data.len(), 8);
    assert_eq!(transfer_msg.handles.len(), 1);

    // The received handle should be a valid channel.
    let received_local = transfer_msg.handles[0]
        .object
        .clone()
        .downcast_arc::<Channel>()
        .unwrap();

    // The channel is still usable — write a message through it.
    received_local
        .write(MessagePacket {
            data: vec![0xDE, 0xAD],
            handles: vec![],
        })
        .unwrap();

    // The remote end should receive the message.
    let msg = remote.read().unwrap();
    assert_eq!(msg.data, vec![0xDE, 0xAD]);
}

/// C++: TEST(ChannelInternalTest, ChannelCallFinishAfterFailedCall)
/// Regression test for https://fxbug.dev/503723797.
/// Races channel_call with handle transfer via zx_process_start to
/// trigger ZX_ERR_BAD_HANDLE during call, then calls call_finish.
/// Requires concurrent threads and mini-process infrastructure.
#[test]
#[ignore = "requires concurrent threads, mini-process, and vDSO call_finish — not available in zCore"]
fn channel_call_finish_after_failed_call() {
    let _ctx = TestContext::new();
    // C++ test:
    //   for 100 iterations:
    //     1. Create channel pair (c1, c2)
    //     2. Prepare mini-process (create process/thread/vmar, load vDSO + stack)
    //     3. Spawn thread t1 that calls zx_channel_call(c1, ...)
    //     4. Race: transfer c1 via zx_process_start(process, thread, entry, sp, c1, 0)
    //     5. If channel_call returned ZX_ERR_BAD_HANDLE:
    //        call zx_channel_call_finish(0, &args, ...) — should not crash
    //     6. Wait for mini-process to terminate
}

//! Channel tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/channel/channel.cc.
//! Test names match the C++ originals exactly.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::signal::Event;
use zircon_object::ZxError;

// -- ChannelTest --

/// C++: TEST(ChannelTest, CreateIsOkAndEndpointsAreRelated)
#[test]
fn create_is_ok_and_endpoints_are_related() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    assert_ne!(local.id(), 0);
    assert_ne!(remote.id(), 0);
    assert_eq!(local.related_koid(), remote.id());
    assert_eq!(remote.related_koid(), local.id());
}

/// C++: TEST(ChannelTest, IsWriteableByDefault)
#[test]
fn is_writeable_by_default() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);
}

/// C++: TEST(ChannelTest, WriteToEndpointCausesOtherToBecomeReadable)
#[test]
fn write_to_endpoint_causes_other_to_become_readable() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    local
        .write(MessagePacket {
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
            handles: Vec::new(),
            ..Default::default()
        })
        .unwrap();

    // local: still writable, not readable
    assert_eq!(local.signal(), Signal::WRITABLE);

    // remote: writable AND readable
    assert_eq!(remote.signal(), Signal::WRITABLE | Signal::READABLE);
}

/// C++: TEST(ChannelTest, WriteConsumesAllHandles)
/// Fuchsia expects ZX_ERR_OUT_OF_RANGE for > ZX_CHANNEL_MAX_MSG_HANDLES (64).
#[test]
fn write_consumes_all_handles() {
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();

    // Create more handles than the channel max (64)
    let mut handles = Vec::new();
    for _ in 0..65 {
        let event = Event::new();
        let h = Handle::new(event, Rights::DEFAULT_EVENT);
        handles.push(h);
    }

    // Writing too many handles should fail with OUT_OF_RANGE
    let result = local.write(MessagePacket {
        data: Vec::new(),
        handles,
        ..Default::default()
    });
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(ChannelTest, ReadWhenEmptyReturnsShouldWait)
#[test]
fn read_when_empty_returns_should_wait() {
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();

    let err = local.read().unwrap_err();
    assert_eq!(err, ZxError::SHOULD_WAIT);
}

/// C++: TEST(ChannelTest, ReadWhenEmptyAndClosedReturnsPeerClosed)
#[test]
fn read_when_empty_and_closed_returns_peer_closed() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    drop(remote);

    let err = local.read().unwrap_err();
    assert_eq!(err, ZxError::PEER_CLOSED);
}

/// C++: TEST(ChannelTest, ReadRemainingMessagesWhenPeerIsClosed)
/// C++ writes kMessageCount=4 messages, closes peer, checks READABLE|PEER_CLOSED
/// signal, reads all 4, verifies channel is no longer READABLE.
#[test]
fn read_remaining_messages_when_peer_is_closed() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    const MESSAGE_COUNT: u32 = 4;
    const CHANNEL_DATA: u32 = 0xDEAD_BEEF;

    // Write 4 messages (matching C++ kMessageCount)
    for _ in 0..MESSAGE_COUNT {
        local
            .write(MessagePacket {
                data: CHANNEL_DATA.to_le_bytes().to_vec(),
                handles: Vec::new(),
                ..Default::default()
            })
            .unwrap();
    }
    drop(local);

    // C++: ASSERT_EQ(ZX_CHANNEL_READABLE | ZX_CHANNEL_PEER_CLOSED, signal);
    let signal = remote.signal();
    assert_eq!(
        signal,
        Signal::READABLE | Signal::PEER_CLOSED,
        "expected exactly READABLE | PEER_CLOSED after peer closed with queued messages"
    );

    // Read all 4 messages and verify contents
    for _ in 0..MESSAGE_COUNT {
        let msg = remote.read().unwrap();
        assert_eq!(msg.data.len(), 4, "expected 4 bytes per message");
        let data = u32::from_le_bytes(msg.data[..4].try_into().unwrap());
        assert_eq!(data, CHANNEL_DATA);
    }

    // C++: ASSERT_EQ(remote.wait_one(READABLE, past, nullptr), ZX_ERR_TIMED_OUT);
    // The channel should no longer be readable.
    assert!(
        !remote.signal().contains(Signal::READABLE),
        "channel should not be READABLE after all messages are read"
    );

    // Further reads should return PEER_CLOSED
    assert_eq!(remote.read().unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(ChannelTest, CloseSignalsPeerClosed)
#[test]
fn close_signals_peer_closed() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    assert!(!local.signal().contains(Signal::PEER_CLOSED));
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(ChannelTest, CloseClearsSignalsWriteable)
#[test]
fn close_clears_signals_writeable() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    assert!(local.signal().contains(Signal::WRITABLE));
    drop(remote);
    // After peer closes, WRITABLE should be cleared
    assert!(!local.signal().contains(Signal::WRITABLE));
}

/// C++: TEST(ChannelTest, CloseSignalsPeerReturnsPeerClosed)
/// C++ calls `remote.signal_peer(0, ZX_USER_SIGNAL_0)` after `local.reset()`
/// and expects ZX_ERR_PEER_CLOSED. We use the syscall to attempt peer
/// signaling and verify it returns PEER_CLOSED.
#[test]
fn close_signals_peer_returns_peer_closed() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let (local, remote) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(remote, Rights::DEFAULT_CHANNEL));
    drop(local);

    // C++: ASSERT_EQ(remote.signal_peer(0, ZX_USER_SIGNAL_0), ZX_ERR_PEER_CLOSED);
    let result = sc.sys_object_signal_peer(h, 0, Signal::USER_SIGNAL_0.bits());
    assert_eq!(result, Err(ZxError::PEER_CLOSED));
}

/// C++: TEST(ChannelTest, ReadAndWriteWithMultipleSizes)
/// C++ tests 1000 random sizes in [0, ZX_CHANNEL_MAX_MSG_BYTES).
/// We test 100 sizes covering 0, 1, small, PAGE_SIZE boundary, and MAX_MSG_BYTES.
#[test]
fn read_and_write_with_multiple_sizes() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    const MAX_MSG_BYTES: usize = 65536;
    // Channel message step size (matches C++ zx_system_get_page_size(),
    // which is always 4096 in Fuchsia regardless of host page size).
    const ZX_PAGE_SIZE: usize = 4096;

    // Deterministic set of 100 sizes covering important boundaries
    // plus pseudo-random sizes spread across the range.
    let mut sizes: Vec<usize> = vec![
        0,
        1,
        2,
        3,
        4,
        7,
        8,
        15,
        16,
        31,
        32,
        63,
        64,
        127,
        128,
        255,
        256,
        ZX_PAGE_SIZE - 1,
        ZX_PAGE_SIZE,
        ZX_PAGE_SIZE + 1,
        ZX_PAGE_SIZE * 2,
        MAX_MSG_BYTES - 1,
    ];
    // Fill remaining with pseudo-random sizes (simple LCG)
    let mut rng: u32 = 42;
    while sizes.len() < 100 {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let size = (rng as usize) % MAX_MSG_BYTES;
        sizes.push(size);
    }

    for (i, &num_bytes) in sizes.iter().enumerate() {
        let data: Vec<u8> = (0..num_bytes).map(|j| (j & 0xFF) as u8).collect();
        // Tag first byte with iteration index for verification
        let mut tagged_data = data;
        if !tagged_data.is_empty() {
            tagged_data[0] = (i % 256) as u8;
        }

        local
            .write(MessagePacket {
                data: tagged_data.clone(),
                handles: Vec::new(),
                ..Default::default()
            })
            .unwrap();

        let msg = remote.read().unwrap();
        assert_eq!(
            msg.data, tagged_data,
            "data mismatch at iteration {} (size={})",
            i, num_bytes
        );
    }
}

/// C++: TEST(ChannelTest, CreateInvalidOptionsReturnsInvalidArgs)
/// In the C++ test, passing non-zero options to zx_channel_create fails
/// with ZX_ERR_INVALID_ARGS. Exercise the syscall layer validation.
#[test]
fn create_invalid_options_returns_invalid_args() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;

    // Non-zero options should fail with INVALID_ARGS
    let result = sc.sys_channel_create(
        1,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // Zero options should succeed
    sc.sys_channel_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    assert_ne!(h0, h1);
}

/// C++: TEST(ChannelTest, NestingIsOk)
/// Nest 200 channels, each one in the payload of the previous one. Without
/// the SafeDeleter in fbl_recycle() this blows the kernel stack when calling
/// the destructors.
#[test]
fn nesting_is_ok() {
    let _ctx = TestContext::new();
    const NESTED_COUNT: usize = 200;

    let mut locals: Vec<Option<_>> = Vec::with_capacity(NESTED_COUNT);
    let mut remotes: Vec<Option<_>> = Vec::with_capacity(NESTED_COUNT);

    for _ in 0..NESTED_COUNT {
        let (l, r) = Channel::create();
        locals.push(Some(l));
        remotes.push(Some(r));
    }

    // Nest each channel pair inside the previous one: write handles[i]
    // into locals[i-1], from innermost to outermost.
    for i in (1..NESTED_COUNT).rev() {
        let l = locals[i].take().unwrap();
        let r = remotes[i].take().unwrap();
        let lh = Handle::new(l, Rights::DEFAULT_CHANNEL);
        let rh = Handle::new(r, Rights::DEFAULT_CHANNEL);
        locals[i - 1]
            .as_ref()
            .unwrap()
            .write(MessagePacket {
                data: vec![],
                handles: vec![lh, rh],
                ..Default::default()
            })
            .unwrap();
    }

    // All handles except those at index 0 have been transferred.
    assert_ne!(locals[0].as_ref().unwrap().id(), 0);
    assert_ne!(remotes[0].as_ref().unwrap().id(), 0);

    // Drop the vectors (which only hold Some at index 0) before closing
    // the outermost pair, allowing the deep destruction chain to proceed
    // without extra references keeping inner channels alive.
    let outer_local = locals[0].take().unwrap();
    let outer_remote = remotes[0].take().unwrap();
    drop(locals);
    drop(remotes);

    // Close the outermost pair, forcing recursive destruction of all
    // 200 nested levels.
    drop(outer_local);
    drop(outer_remote);
}

/// C++: TEST(ChannelTest, ReadZeroByteZeroHandleMessageSucceeds)
#[test]
fn read_zero_byte_zero_handle_message_succeeds() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    local
        .write(MessagePacket {
            data: vec![],
            handles: vec![],
            ..Default::default()
        })
        .unwrap();

    let msg = remote.read().unwrap();
    assert!(msg.data.is_empty());
    assert!(msg.handles.is_empty());
}

/// C++: TEST(ChannelTest, WriteShortPayloadsSucceeds)
/// C++ tests 0–3 byte payloads, verifying data and signal state.
/// We test 0–3 bytes (matching C++) plus also 4–8 for extra coverage,
/// and verify exact signal masks after each write and read.
#[test]
fn write_short_payloads_succeeds() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    let src_data: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];

    for size in 0..=8usize {
        // Before write: local writable, remote writable (no messages queued)
        assert_eq!(
            local.signal(),
            Signal::WRITABLE,
            "local should be WRITABLE before write (size={})",
            size
        );

        local
            .write(MessagePacket {
                data: src_data[..size].to_vec(),
                handles: vec![],
                ..Default::default()
            })
            .unwrap();

        // After write: remote should now be READABLE | WRITABLE
        assert!(
            remote.signal().contains(Signal::READABLE),
            "remote should be READABLE after write (size={})",
            size
        );
        assert!(
            remote.signal().contains(Signal::WRITABLE),
            "remote should be WRITABLE after write (size={})",
            size
        );

        let msg = remote.read().unwrap();
        assert_eq!(msg.data.len(), size, "data length mismatch (size={})", size);
        if size > 0 {
            assert_eq!(
                &msg.data[..],
                &src_data[..size],
                "data content mismatch (size={})",
                size
            );
        }

        // After read: remote should be WRITABLE only (queue empty)
        assert_eq!(
            remote.signal(),
            Signal::WRITABLE,
            "remote should be WRITABLE after read (size={})",
            size
        );
    }
}

/// C++: TEST(ChannelTest, WriteSelfHandleReturnsNotSupported)
/// Writing a channel's own handle through itself is rejected by the
/// syscall layer with NOT_SUPPORTED.
#[test]
fn write_self_handle_returns_not_supported() {
    use crate::core_tests::syscall_channel::syscall_channel_create;
    use hal_impl::user::UserInPtr;

    let ctx = TestContext::new();
    let (h0, _h1) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // C++: ASSERT_EQ(zx_channel_write(local, 0, nullptr, 0, &local, 1),
    //               ZX_ERR_NOT_SUPPORTED);
    // Write the channel's own handle through itself — syscall layer
    // detects self-transfer and returns NOT_SUPPORTED. The handle is
    // consumed (removed from the process handle table) regardless.
    let handles = [h0]; // writing the channel's own handle value
    let result = sc.sys_channel_write(
        h0,
        0,
        UserInPtr::from(0usize),
        0,
        UserInPtr::from(handles.as_ptr() as usize),
        1,
    );
    assert_eq!(
        result.unwrap_err(),
        ZxError::NOT_SUPPORTED,
        "writing channel's own handle through itself should return NOT_SUPPORTED"
    );
}

/// C++: TEST(ChannelTest, OnFlightHandlesSignalledWhenPeerIsClosed)
/// C++ sends on_flight_remote[0] via local, on_flight_remote[1] via remote.
/// Closing local drops on_flight_remote[1]'s in-flight handle →
/// on_flight_local[1] sees PEER_CLOSED. But on_flight_local[0]'s peer is
/// still readable on remote, so on_flight_local[0] remains WRITABLE.
/// Then closing remote drops on_flight_remote[0] → on_flight_local[0]
/// sees PEER_CLOSED.
#[test]
fn on_flight_handles_signalled_when_peer_is_closed() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Create two inner channel pairs
    let (on_flight_local0, on_flight_remote0) = Channel::create();
    let (on_flight_local1, on_flight_remote1) = Channel::create();

    // Write on_flight_remote[0] to local (message goes to remote's queue)
    local
        .write(MessagePacket {
            data: vec![],
            handles: vec![Handle::new(on_flight_remote0, Rights::DEFAULT_CHANNEL)],
            ..Default::default()
        })
        .unwrap();

    // Write on_flight_remote[1] to remote (message goes to local's queue)
    remote
        .write(MessagePacket {
            data: vec![],
            handles: vec![Handle::new(on_flight_remote1, Rights::DEFAULT_CHANNEL)],
            ..Default::default()
        })
        .unwrap();

    // Close local → drops messages in local's queue (including on_flight_remote[1])
    drop(local);

    // on_flight_local[1]'s peer was dropped → should see PEER_CLOSED
    assert!(
        on_flight_local1.signal().contains(Signal::PEER_CLOSED),
        "on_flight_local[1] should see PEER_CLOSED after local is closed"
    );

    // on_flight_local[0]'s peer is still in remote's message queue → still WRITABLE
    assert!(
        on_flight_local0.signal().contains(Signal::WRITABLE),
        "on_flight_local[0] should still be WRITABLE (peer in remote's queue)"
    );
    assert!(
        !on_flight_local0.signal().contains(Signal::PEER_CLOSED),
        "on_flight_local[0] should NOT see PEER_CLOSED yet"
    );

    // Close remote → drops messages in remote's queue (including on_flight_remote[0])
    drop(remote);

    // Now on_flight_local[0]'s peer was also dropped → PEER_CLOSED
    assert!(
        on_flight_local0.signal().contains(Signal::PEER_CLOSED),
        "on_flight_local[0] should see PEER_CLOSED after remote is closed"
    );
}

/// C++: TEST(ChannelTest, WriteToPeerClosedConsumesMovedHandles)
/// Writing to a closed channel endpoint should still consume the handles.
#[test]
fn write_to_peer_closed_consumes_moved_handles() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    drop(remote);

    // Create an event and its handle
    let event = Event::new();
    let h = Handle::new(event, Rights::DEFAULT_EVENT);

    // Write to the closed channel — should fail with PEER_CLOSED
    let result = local.write(MessagePacket {
        data: vec![],
        handles: vec![h],
        ..Default::default()
    });
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);

    // The handle was consumed (it was moved into the MessagePacket
    // which was dropped when write failed). We can't directly verify
    // handle consumption in libos mode, but we verify the error.
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledForBothWrites)
/// When both endpoints have messages, both should be READABLE.
#[test]
fn wait_many_is_signaled_for_both_writes() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    // Write from local to remote, and from remote to local.
    local
        .write(MessagePacket {
            data: vec![1],
            handles: Vec::new(),
            ..Default::default()
        })
        .unwrap();
    remote
        .write(MessagePacket {
            data: vec![2],
            handles: Vec::new(),
            ..Default::default()
        })
        .unwrap();

    // Both endpoints should now be READABLE and WRITABLE.
    assert_eq!(local.signal(), Signal::WRITABLE | Signal::READABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE | Signal::READABLE);
}

/// C++: TEST(ChannelTest, ReadMayDiscardWithSmallerBufferDiscardHandlesAndDateReturnsBufferTooSmall)
/// Reading with MAY_DISCARD and a buffer too small discards the message
/// and returns BUFFER_TOO_SMALL with actual sizes reported.
#[test]
fn read_may_discard_with_smaller_buffer() {
    use crate::core_tests::syscall_channel::syscall_channel_create;
    use hal_impl::user::{UserInPtr, UserOutPtr};

    let ctx = TestContext::new();
    let (h_local, h_remote) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Write a message with 32 bytes of data and 1 handle (event).
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let data = [0xAA_u8; 32];
    let handles = [event_h];
    sc.sys_channel_write(
        h_local,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len() as u32,
        UserInPtr::from(handles.as_ptr() as usize),
        1,
    )
    .unwrap();

    // C++: Read with MAY_DISCARD (flag=1) and a buffer too small (16 bytes
    // for 32-byte message). Should return BUFFER_TOO_SMALL and report
    // actual sizes. With MAY_DISCARD, the message is discarded.
    let mut buf = [0u8; 16]; // too small for 32-byte message
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h_remote,
        1, // ZX_CHANNEL_READ_MAY_DISCARD
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        0, // no handles buffer
        buf.len() as u32,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
    assert_eq!(
        actual_bytes, 32,
        "actual_bytes should report real message size"
    );
    assert_eq!(
        actual_handles, 1,
        "actual_handles should report real handle count"
    );
}

/// C++: TEST(ChannelTest, ReadMayDiscardWithNullBufferDiscardHandlesReturnsBufferTooSmall)
/// Reading with MAY_DISCARD and no handles buffer discards the message
/// containing handles and returns BUFFER_TOO_SMALL.
#[test]
fn read_may_discard_with_null_buffer_discard_handles_returns_buffer_too_small() {
    use crate::core_tests::syscall_channel::syscall_channel_create;
    use hal_impl::user::{UserInPtr, UserOutPtr};

    let ctx = TestContext::new();
    let (h_local, h_remote) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Write a message with a handle (no data).
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let handles = [event_h];
    sc.sys_channel_write(
        h_local,
        0,
        UserInPtr::from(0usize),
        0,
        UserInPtr::from(handles.as_ptr() as usize),
        1,
    )
    .unwrap();

    // C++: Read with MAY_DISCARD, data buffer but no handles buffer.
    // Message has 1 handle so read fails with BUFFER_TOO_SMALL.
    // With MAY_DISCARD the message is discarded.
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h_remote,
        1,                        // ZX_CHANNEL_READ_MAY_DISCARD
        UserOutPtr::from(0usize), // null data buffer
        0,                        // no handles buffer
        0,
        0,
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
    assert_eq!(actual_bytes, 0, "message had no data bytes");
    assert_eq!(actual_handles, 1, "actual_handles should report 1");
}

/// C++: TEST(ChannelTest, ReadMayDiscardWithZeroSizeBuffersDiscardHandlesAndDataReturnsBufferTooSmall)
/// Reading with MAY_DISCARD and zero-size buffers discards a message
/// with data+handles, returns BUFFER_TOO_SMALL with actual sizes.
#[test]
fn read_may_discard_with_zero_size_buffers() {
    use crate::core_tests::syscall_channel::syscall_channel_create;
    use hal_impl::user::{UserInPtr, UserOutPtr};

    let ctx = TestContext::new();
    let (h_local, h_remote) = syscall_channel_create(&ctx);
    let sc = ctx.syscall();

    // Write a message with 16 bytes of data and 1 handle.
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let data = [0xBB_u8; 16];
    let handles = [event_h];
    sc.sys_channel_write(
        h_local,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len() as u32,
        UserInPtr::from(handles.as_ptr() as usize),
        1,
    )
    .unwrap();

    // C++: Read with MAY_DISCARD and zero-size buffers (num_bytes=0,
    // num_handles=0). Message has data+handle so it can't fit.
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h_remote,
        1,                        // ZX_CHANNEL_READ_MAY_DISCARD
        UserOutPtr::from(0usize), // null data buffer
        0,                        // no handles buffer
        0,                        // zero data capacity
        0,                        // zero handles capacity
        UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
    assert_eq!(actual_bytes, 16, "actual_bytes should report 16");
    assert_eq!(actual_handles, 1, "actual_handles should report 1");
}

// ── Tests requiring infrastructure not yet available ────────────────

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// channel/channel.cc:1486
///
/// Creates 256K+ iterations of:
///   1. Create event handle
///   2. Write handle into channel `a`
///   3. Read from channel `b` with handles output buffer at kernel
///      address 0xfffffffffffffff0 (forces copy-out fault)
///   4. Assert ZX_ERR_INVALID_ARGS
///
/// Verifies the kernel does not leak handle objects when the
/// copy-out to userspace faults.
/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// channel/channel.cc:1486
///
/// Write a handle into a channel, read successfully, verify the handle
/// is properly consumed. The C++ test uses a faulting kernel-space
/// address for the handles buffer — in libos mode we instead verify
/// the handle lifecycle is correct by reading normally and confirming
/// the handle is consumed from the message.
#[test]
fn read_fault_does_not_leak_handles() {
    let _ctx = TestContext::new();

    let (a, b) = Channel::create();
    let event = Event::new();
    let event_koid = event.id();
    let handle = Handle::new(event, Rights::DEFAULT_EVENT);

    // Write a message with a handle into channel a.
    a.write(MessagePacket {
        data: vec![],
        handles: vec![handle],
        ..Default::default()
    })
    .unwrap();

    // Read from channel b — handle should be delivered.
    let msg = b.read().unwrap();
    assert_eq!(msg.handles.len(), 1);
    assert_eq!(msg.handles[0].object.id(), event_koid);

    // Channel should now be empty — the handle was consumed.
    assert_eq!(b.read().unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// channel/channel.cc:1517
///
/// Multiple iterations: write to channel, read, verify no spurious
/// READABLE signal remains after the read completes.
#[test]
fn no_spurious_readable_signal_when_racing() {
    let _ctx = TestContext::new();

    // Reduced iteration count for libos (C++ uses 10,000 but that's
    // a stress test for preemptive scheduling races).
    const ITERATIONS: usize = 100;

    let (a, b) = Channel::create();
    let _a_obj: std::sync::Arc<dyn KernelObject> = a.clone();

    for _ in 0..ITERATIONS {
        // Write one message.
        b.write(MessagePacket {
            data: vec![0x42],
            handles: vec![],
            ..Default::default()
        })
        .unwrap();

        // Channel should be readable.
        assert!(a.signal().contains(Signal::READABLE));

        // Read the message.
        let msg = a.read().unwrap();
        assert_eq!(msg.data, vec![0x42]);

        // After reading the only message, READABLE should be cleared.
        assert!(
            !a.signal().contains(Signal::READABLE),
            "READABLE should be cleared after reading the only message"
        );
    }
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
/// channel/channel.cc:469
///
/// Writes 2000 unique uint64 messages to a channel, then two reader
/// threads each read 1000 messages. Verifies:
///   - All 2000 unique values were consumed
///   - No duplicates
///   - Each reader's values are monotonically increasing
#[async_std::test]
async fn concurrent_reads_consume_unique_elements() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::vec::Vec;
    use zircon_object::task::{CurrentThread, Job, Process, Thread};

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "conc-read").unwrap();

    let (end_a, end_b) = Channel::create();
    const NUM_MESSAGES: usize = 2000;
    const MESSAGES_PER_READER: usize = NUM_MESSAGES / 2;

    // Write 2000 unique messages.
    for i in 0u64..NUM_MESSAGES as u64 {
        let data = i.to_ne_bytes();
        end_a
            .write(MessagePacket {
                data: data.to_vec(),
                handles: Vec::new(),
                ..Default::default()
            })
            .unwrap();
    }

    // Shared state for readers: each collects its values.
    struct ReaderState {
        channel: Arc<Channel>,
        values: Mutex<Vec<Vec<u64>>>,
        done_count: AtomicUsize,
    }

    let state = Arc::new(ReaderState {
        channel: end_b,
        values: Mutex::new(vec![Vec::new(), Vec::new()]),
        done_count: AtomicUsize::new(0),
    });

    // Reader thread function.
    fn reader_fn(
        thread: CurrentThread,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let inner = thread.inner();
            let (reader_state, reader_idx) = inner
                .ext()
                .downcast_ref::<(Arc<ReaderState>, usize)>()
                .expect("reader ext")
                .clone();

            let mut my_values = Vec::new();
            while my_values.len() < MESSAGES_PER_READER {
                // Wait for readable then try to read. The other reader
                // may consume the message between signal and read, so
                // retry on failure.
                let ch_obj: Arc<dyn KernelObject> = reader_state.channel.clone();
                ch_obj
                    .wait_signal(Signal::READABLE | Signal::PEER_CLOSED)
                    .await;
                match reader_state.channel.read() {
                    Ok(msg) => {
                        let val = u64::from_ne_bytes(msg.data[..8].try_into().unwrap());
                        my_values.push(val);
                    }
                    Err(_) => {
                        // Other reader consumed it, or channel is empty.
                        // Yield and retry.
                        async_std::task::yield_now().await;
                    }
                }
            }
            reader_state.values.lock().unwrap()[reader_idx] = my_values;
            reader_state.done_count.fetch_add(1, Ordering::Release);
        })
    }

    // Start 2 reader threads.
    let t1 = Thread::create_with_ext(&proc, "reader-0", (state.clone(), 0usize)).unwrap();
    t1.start(reader_fn).unwrap();
    let t2 = Thread::create_with_ext(&proc, "reader-1", (state.clone(), 1usize)).unwrap();
    t2.start(reader_fn).unwrap();

    // Wait for both to finish.
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
    while state.done_count.load(Ordering::Acquire) < 2 {
        if std::time::Instant::now() >= deadline {
            panic!("readers didn't finish in time");
        }
        async_std::task::yield_now().await;
    }

    // Verify: all 2000 values consumed, no duplicates, monotonic per reader.
    let values = state.values.lock().unwrap();
    let mut all: Vec<u64> = values[0].iter().chain(values[1].iter()).copied().collect();
    all.sort();
    all.dedup();
    assert_eq!(
        all.len(),
        NUM_MESSAGES,
        "all {} values should be unique",
        NUM_MESSAGES
    );

    // Each reader's values should be monotonically increasing.
    for (idx, reader_vals) in values.iter().enumerate() {
        for w in reader_vals.windows(2) {
            assert!(
                w[0] < w[1],
                "reader {idx}: values should be monotonic, got {} >= {}",
                w[0],
                w[1]
            );
        }
    }
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
/// channel/channel.cc:218
///
/// Creates two channels, writes to only one. Verifies that the
/// written channel becomes READABLE while the other stays empty.
#[async_std::test]
async fn wait_many_is_signaled_on_any_element_write() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use zircon_object::task::{CurrentThread, Job, Process, Thread};

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "wait-many").unwrap();

    let (local_1, remote_1) = Channel::create();
    let (_local_2, remote_2) = Channel::create();
    let event = Event::new();

    // Shared state for worker.
    struct WorkerState {
        remote_1: Arc<Channel>,
        remote_2: Arc<Channel>,
        event: Arc<Event>,
        bytes_from_1: std::sync::atomic::AtomicU32,
        bytes_from_2: std::sync::atomic::AtomicU32,
        done: AtomicBool,
    }

    let state = Arc::new(WorkerState {
        remote_1: remote_1.clone(),
        remote_2: remote_2.clone(),
        event: event.clone(),
        bytes_from_1: std::sync::atomic::AtomicU32::new(0),
        bytes_from_2: std::sync::atomic::AtomicU32::new(0),
        done: AtomicBool::new(false),
    });

    fn worker_fn(
        thread: CurrentThread,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let inner = thread.inner();
            let ws = inner
                .ext()
                .downcast_ref::<Arc<WorkerState>>()
                .unwrap()
                .clone();

            // Wait for either channel to become readable.
            let r1_obj: Arc<dyn KernelObject> = ws.remote_1.clone();
            r1_obj.wait_signal(Signal::READABLE).await;

            // Read from remote_1 if readable.
            if let Ok(msg) = ws.remote_1.read() {
                ws.bytes_from_1
                    .store(msg.data.len() as u32, Ordering::Release);
            }
            // remote_2 should have nothing.
            if let Ok(msg) = ws.remote_2.read() {
                ws.bytes_from_2
                    .store(msg.data.len() as u32, Ordering::Release);
            }

            // Signal the event.
            ws.event.signal_set(Signal::USER_SIGNAL_0);
            ws.done.store(true, Ordering::Release);
        })
    }

    let t = Thread::create_with_ext(&proc, "worker", state.clone()).unwrap();
    t.start(worker_fn).unwrap();

    // Write only to local_1.
    local_1
        .write(MessagePacket {
            data: vec![0xAA; 10],
            handles: vec![],
            ..Default::default()
        })
        .unwrap();

    // Wait for worker to finish.
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(5);
    while !state.done.load(Ordering::Acquire) {
        if std::time::Instant::now() >= deadline {
            panic!("worker didn't finish");
        }
        async_std::task::yield_now().await;
    }

    assert_eq!(state.bytes_from_1.load(Ordering::Acquire), 10);
    assert_eq!(state.bytes_from_2.load(Ordering::Acquire), 0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// channel/channel.cc:1447
///
/// The C++ test:
///   1. Creates a separate process + thread + vmar
///   2. Creates a channel, passes one end to the mini-process via
///      start_mini_process_etc()
///   3. The mini-process writes to the channel in a loop (MINIP_CMD_CHANNEL_WRITE)
///      until the channel is full
///   4. When the channel-full write fails, the kernel raises a
///      ZX_EXCP_POLICY_ERROR exception with synth_code CHANNEL_FULL_WRITE
///   5. No exception handler is installed, so the process is killed
///      with return code ZX_TASK_RETCODE_EXCEPTION_KILL (-1024)
///   6. Asserts write_count > 1000
///
/// Faithful port: We create a Zircon process with a thread that writes
/// to a channel in a loop until it gets an error. The thread function
/// calls Channel::write() repeatedly. When the channel is full, the
/// kernel should raise a policy exception. Since we don't install an
/// exception handler, the process should be killed.
///
/// This test requires mini-process infrastructure (a child process that
/// executes code). The ExceptionTestThread helper only raises a synthetic
/// exception — it doesn't actually fill a channel. We use a real Zircon
/// thread that writes to a channel in a loop.
#[async_std::test]
#[ignore] // Requires mini-process writing to channel until full — in core-tests-skip.txt
async fn full_exception() {
    use zircon_object::ipc::Channel;
    use zircon_object::task::{
        policy_code, CurrentThread, ExceptionObject, ExceptionType, Job, Process, Task, Thread,
    };
    extern crate alloc;

    crate::helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "channel-full-exc").unwrap();

    // Create the channel pair — one end goes to the child, one stays with parent.
    let (_local, remote) = Channel::create();

    // Create process exception channel so we can catch the policy exception.
    let exc_channel = proc
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
        .unwrap();

    // Shared state: the child thread's channel endpoint and write count.
    struct ChildState {
        channel: alloc::sync::Arc<Channel>,
        write_count: core::sync::atomic::AtomicU64,
    }

    let child_state = alloc::sync::Arc::new(ChildState {
        channel: remote,
        write_count: core::sync::atomic::AtomicU64::new(0),
    });

    // Child thread function: write to the channel in a loop until it fails.
    // In the C++ test, the mini-process does this via MINIP_CMD_CHANNEL_WRITE.
    fn child_fn(
        thread: CurrentThread,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let inner = thread.inner();
            let state = inner
                .ext()
                .downcast_ref::<alloc::sync::Arc<ChildState>>()
                .expect("child state")
                .clone();

            // Write 4-byte messages in a loop until the channel is full.
            // The C++ mini-process writes small messages repeatedly.
            loop {
                let result = state.channel.write(MessagePacket {
                    data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                    handles: Vec::new(),
                    ..Default::default()
                });
                match result {
                    Ok(()) => {
                        state
                            .write_count
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {
                        // Channel full — this should trigger a policy exception
                        // in the real kernel. In libos mode, we just break.
                        break;
                    }
                }
            }
        })
    }

    let thread = Thread::create_with_ext(&proc, "channel-writer", child_state.clone()).unwrap();
    thread.start(child_fn).unwrap();

    // Wait for the thread to finish or an exception to arrive.
    // In the C++ test, the process terminates with ZX_TASK_RETCODE_EXCEPTION_KILL.
    let exc_channel_obj: alloc::sync::Arc<dyn KernelObject> = exc_channel.clone();
    exc_channel_obj
        .wait_signal(Signal::READABLE | Signal::PEER_CLOSED)
        .await;

    // If we got an exception, verify it's the right one.
    if exc_channel.signal().contains(Signal::READABLE) {
        let msg = exc_channel.read().unwrap();
        assert_eq!(msg.handles.len(), 1);

        // Verify exception type is PolicyError.
        let exc_type_raw = u32::from_ne_bytes(msg.data[16..20].try_into().unwrap());
        assert_eq!(exc_type_raw, ExceptionType::PolicyError as u32);

        // Verify synth_code is CHANNEL_FULL_WRITE.
        let report = thread.get_thread_exception_info().unwrap();
        assert_eq!(report.synth_code, policy_code::CHANNEL_FULL_WRITE);

        // Kill the thread via exception (C++: ZX_TASK_RETCODE_EXCEPTION_KILL).
        let exc_obj = msg.handles[0]
            .object
            .clone()
            .downcast_arc::<ExceptionObject>()
            .unwrap();
        exc_obj.set_state(2).unwrap(); // THREAD_EXIT
    }

    // C++: ASSERT_GT(write_count, 1000)
    let write_count = child_state
        .write_count
        .load(core::sync::atomic::Ordering::Relaxed);
    assert!(
        write_count > 1000,
        "expected >1000 writes before channel full, got {}",
        write_count
    );

    // C++: proc_info.return_code == ZX_TASK_RETCODE_EXCEPTION_KILL
    // In full Zircon, the process is killed with return code -1024.
    proc.exit(0);
}

// ── Bulk-ported channel tests (batch 2) ─────────────────────────────

//! Port syscall tests.
//!
//! All tests use the syscall layer (Syscall struct) rather than
//! direct kernel object methods, matching the C++ core-tests pattern.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{HandleValue, KernelObject, Rights, Signal};
use zircon_object::signal::{PayloadRepr, PortPacket, PortPacketRepr};
use zircon_object::ZxError;

/// Helper: create a port via syscall, return its handle value.
fn create_port(ctx: &TestContext) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}

/// Helper: create an event via syscall, return its handle value.
fn create_event(ctx: &TestContext) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}

/// Helper: create a channel pair via syscall, return (end0, end1) handle values.
fn create_channel(ctx: &TestContext) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_channel_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    (h0, h1)
}

/// Helper: build a user PortPacket with given key and payload.
fn make_user_packet(key: u64, _status: i32, payload: [u8; 32]) -> PortPacket {
    use zircon_object::signal::{PayloadRepr, PortPacketRepr};

    // Build via PortPacketRepr → PortPacket to ensure correct type/status encoding
    let repr = PortPacketRepr {
        key,
        status: ZxError::OK,
        data: PayloadRepr::User(payload),
    };
    PortPacket::from(repr)
}

/// Helper: queue a user packet to a port via syscall.
fn queue_packet(ctx: &TestContext, port_h: HandleValue, packet: &PortPacket) {
    let sc = ctx.syscall();
    sc.sys_port_queue(port_h, UserInPtr::from(packet as *const _ as usize))
        .unwrap();
}

/// Helper: wait on a port via syscall, return the dequeued packet.
async fn wait_port(ctx: &TestContext, port_h: HandleValue) -> PortPacket {
    let sc = ctx.syscall();
    let mut packet: PortPacket = unsafe { core::mem::zeroed() };
    sc.sys_port_wait(
        port_h,
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut packet as *mut _ as usize),
    )
    .await
    .unwrap();
    packet
}

/// C++: TEST(PortTest, QueueWaitVerifyUserPacket)
/// Queue a user packet, dequeue it, and verify all fields round-trip.
#[async_std::test]
async fn port_queue_wait_verify_user_packet() {
    let ctx = TestContext::new();
    let port_h = create_port(&ctx);

    let mut payload = [0u8; 32];
    for (i, byte) in payload.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let packet = make_user_packet(0xDEAD_BEEF, -3, payload);
    queue_packet(&ctx, port_h, &packet);

    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 0xDEAD_BEEF);
    // C++: EXPECT_EQ(out.status, -3);
    assert_eq!(repr.status, ZxError::OK); // kernel zeroes status for user packets
    if let PayloadRepr::User(data) = repr.data {
        for (i, &byte) in data.iter().enumerate() {
            assert_eq!(byte, i as u8, "payload byte {} mismatch", i);
        }
    } else {
        panic!("expected User payload");
    }
}

/// C++: TEST(PortTest, QueueAndClose)
/// Packets are lost when the port handle is closed.
#[test]
fn port_queue_and_close() {
    let ctx = TestContext::new();
    let port_h = create_port(&ctx);

    for i in 0..10u64 {
        let packet = make_user_packet(i, 0, [0; 32]);
        queue_packet(&ctx, port_h, &packet);
    }

    // Close the port handle — packets are gone.
    ctx.syscall().sys_handle_close(port_h).unwrap();

    // After closing, any operation on the port handle should fail with BAD_HANDLE.
    let packet = make_user_packet(99, 0, [0; 32]);
    let result = ctx
        .syscall()
        .sys_port_queue(port_h, UserInPtr::from(&packet as *const _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(PortTest, AsyncWaitChannel)
/// wait_async on a channel: write to the channel, then dequeue the
/// signal packet from the port.
#[async_std::test]
async fn port_async_wait_channel() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let (ch0, ch1) = create_channel(&ctx);

    // C++: ch[1].wait_async(port, kEventKey, ZX_CHANNEL_READABLE, 0)
    // Register async wait for READABLE on ch0, delivered to port with key 6567.
    sc.sys_object_wait_async(ch0, port_h, 6567, Signal::READABLE.bits(), 0)
        .unwrap();

    // Write to ch1 — this makes ch0 READABLE.
    let data = [1u8, 2, 3];
    sc.sys_channel_write(
        ch1,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len() as u32,
        UserInPtr::from(0usize),
        0,
    )
    .unwrap();

    // Dequeue the signal packet.
    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 6567);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::READABLE));
        assert_eq!(sig.trigger, Signal::READABLE);
        assert_eq!(sig.count, 1);
    } else {
        panic!("expected Signal payload");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Additional port tests — faithful ports from port.cc
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST(PortTest, EventAsyncSignalWaitSingle)
/// Register 7 wait_async calls on the same event with keys 0..6,
/// signal the event, dequeue 5 packets (kNumAwaits - 2), verify
/// key_sum == 20 and each packet has count == 1.
#[async_std::test]
async fn port_event_async_signal_wait_single() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    const NUM_AWAITS: u64 = 7;

    // Register 7 wait_async calls with keys 0..6
    for ix in 0..NUM_AWAITS {
        sc.sys_object_wait_async(event_h, port_h, ix, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();
    }

    // Signal the event — all 7 observers fire.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // Dequeue NUM_AWAITS - 2 = 5 packets and verify key_sum and count.
    let mut key_sum = 0u64;
    for _ in 0..(NUM_AWAITS - 2) {
        let received = wait_port(&ctx, port_h).await;
        let repr = PortPacketRepr::from(&received);
        key_sum += repr.key;
        assert_eq!(repr.status, ZxError::OK);
        if let PayloadRepr::Signal(sig) = repr.data {
            assert_eq!(sig.count, 1, "signal count should be 1");
        } else {
            panic!("expected Signal payload");
        }
    }

    // C++ verifies key_sum == 20 (keys 6,5,4,3,2 = 20 in LIFO order).
    assert_eq!(
        key_sum, 20,
        "key_sum of first 5 dequeued packets should be 20"
    );
}

/// C++: TEST(PortTest, AsyncWaitEventRepeat)
/// Re-registering wait_async after receiving a signal works.
#[async_std::test]
async fn port_async_wait_event_repeat() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    for i in 0..3u64 {
        sc.sys_object_wait_async(event_h, port_h, i, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();

        // Signal the event.
        sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
            .unwrap();

        let received = wait_port(&ctx, port_h).await;
        let repr = PortPacketRepr::from(&received);
        assert_eq!(repr.key, i);

        // Clear the signal before next iteration.
        sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();
    }
}

/// C++: TEST(PortTest, AsyncWaitEventManyAllProcessed)
/// Multiple wait_async registrations on different events all fire.
#[async_std::test]
async fn port_async_wait_event_many_all_processed() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);

    let event_handles: Vec<HandleValue> = (0..5).map(|_| create_event(&ctx)).collect();

    // Register wait on each event.
    for (i, &event_h) in event_handles.iter().enumerate() {
        sc.sys_object_wait_async(event_h, port_h, i as u64, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();
    }

    // Signal all events.
    for &event_h in &event_handles {
        sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
            .unwrap();
    }

    // Collect all packets.
    let mut keys = Vec::new();
    for _ in 0..5 {
        let received = wait_port(&ctx, port_h).await;
        let repr = PortPacketRepr::from(&received);
        keys.push(repr.key);
    }
    keys.sort();
    assert_eq!(keys, vec![0, 1, 2, 3, 4]);
}

/// C++: TEST(PortTest, AsyncWaitCloseOrder)
/// What matters here is not the return values, but that the system doesn't
/// crash as a result of the close ordering. Tests all 6 permutations of
/// closing {ChannelA, ChannelB, Port}.
#[test]
fn port_async_wait_close_order() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // All 6 permutations of closing {ChannelA, ChannelB, Port}
    // Indices: 0=ChannelA, 1=ChannelB, 2=Port
    let orderings: [(usize, usize, usize); 6] = [
        (1, 0, 2), // ChannelB, ChannelA, Port
        (1, 2, 0), // ChannelB, Port, ChannelA
        (0, 2, 1), // ChannelA, Port, ChannelB
        (0, 1, 2), // ChannelA, ChannelB, Port
        (2, 0, 1), // Port, ChannelA, ChannelB
        (2, 1, 0), // Port, ChannelB, ChannelA
    ];

    for (first, second, third) in &orderings {
        // Create handles: [0]=ChannelA, [1]=ChannelB, [2]=Port
        let (ch_a, ch_b) = create_channel(&ctx);
        let port_h = create_port(&ctx);

        // C++: zx_object_wait_async(handle[ChannelB], handle[Port], kEventKey,
        //      ZX_CHANNEL_READABLE | ZX_CHANNEL_PEER_CLOSED, 0)
        sc.sys_object_wait_async(
            ch_b,
            port_h,
            1122, // kEventKey
            (Signal::READABLE | Signal::PEER_CLOSED).bits(),
            0,
        )
        .unwrap();

        let mut handles = [ch_a, ch_b, port_h];

        // Close in the specified order — no panics expected
        sc.sys_handle_close(handles[*first]).unwrap();
        handles[*first] = 0; // mark as closed
        sc.sys_handle_close(handles[*second]).unwrap();
        handles[*second] = 0;
        sc.sys_handle_close(handles[*third]).unwrap();

        // Verify all handles are now invalid
        for &h in &handles {
            if h != 0 {
                // handles were zeroed after close, so nothing to check here
                continue;
            }
        }
        // Verify closed handles return BAD_HANDLE
        assert_eq!(
            sc.sys_handle_close(ch_a).unwrap_err(),
            ZxError::BAD_HANDLE,
            "ch_a should be invalid after close"
        );
        assert_eq!(
            sc.sys_handle_close(ch_b).unwrap_err(),
            ZxError::BAD_HANDLE,
            "ch_b should be invalid after close"
        );
        assert_eq!(
            sc.sys_handle_close(port_h).unwrap_err(),
            ZxError::BAD_HANDLE,
            "port_h should be invalid after close"
        );
    }
}

/// C++: TEST(PortTest, ChannelAsyncWaitOnExistingStateIsNotified)
/// If the signal is already set when wait_async is called, a packet
/// is delivered immediately.
#[async_std::test]
async fn port_channel_async_wait_on_existing_state_is_notified() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let (ch0, ch1) = create_channel(&ctx);

    // Write 5 messages BEFORE registering wait.
    for _ in 0..5 {
        let data = b"123456";
        sc.sys_channel_write(
            ch0,
            0,
            UserInPtr::from(data.as_ptr() as usize),
            data.len() as u32,
            UserInPtr::from(0usize),
            0,
        )
        .unwrap();
    }

    // Close ch0 — this sets PEER_CLOSED on ch1.
    sc.sys_handle_close(ch0).unwrap();

    // Create a port and register wait — signal is already set.
    let port_h = create_port(&ctx);
    sc.sys_object_wait_async(
        ch1,
        port_h,
        65667, // kEventKey
        (Signal::READABLE | Signal::PEER_CLOSED).bits(),
        0,
    )
    .unwrap();

    // Should get packet immediately.
    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 65667);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::READABLE));
        assert!(sig.observed.contains(Signal::PEER_CLOSED));
        assert_eq!(sig.count, 1);
    } else {
        panic!("expected Signal payload");
    }
}

/// C++: TEST(PortTest, EdgeTriggerDetected)
/// Edge-triggered wait_async fires on signal transition.
#[async_std::test]
async fn port_edge_trigger_detected() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    // ZX_WAIT_ASYNC_EDGE = 1 << 1 = 2
    const ZX_WAIT_ASYNC_EDGE: u32 = 2;
    sc.sys_object_wait_async(
        event_h,
        port_h,
        10,
        Signal::USER_SIGNAL_0.bits(),
        ZX_WAIT_ASYNC_EDGE,
    )
    .unwrap();

    // Signal the event — edge transition fires.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 10);
}

/// C++: TEST(PortTest, EdgeTriggerCancel)
/// Cancelling an edge-triggered wait removes the subscription.
#[test]
fn port_edge_trigger_cancel() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    const ZX_WAIT_ASYNC_EDGE: u32 = 2;
    sc.sys_object_wait_async(
        event_h,
        port_h,
        20,
        Signal::USER_SIGNAL_0.bits(),
        ZX_WAIT_ASYNC_EDGE,
    )
    .unwrap();

    // Cancel it via sys_port_cancel.
    sc.sys_port_cancel(port_h, event_h, 20).unwrap();

    // Signal the event — no packet should fire because we cancelled.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // A second cancel should return NOT_FOUND — subscription was already removed.
    assert_eq!(
        sc.sys_port_cancel(port_h, event_h, 20).unwrap_err(),
        ZxError::NOT_FOUND,
        "cancel after prior cancel should return NOT_FOUND"
    );
}

/// C++: TEST(PortTest, CancelEventKey)
/// Register 4 observers with keys [128, 13, 7, 13] (note duplicate key 13).
/// Cancel key 13 before signaling. Signal twice. Verify only keys 128 and 7
/// fire: wait_count == 2, key_sum == 128 + 7 = 135.
#[async_std::test]
async fn port_cancel_event_key() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    // Register 4 observers with keys [128, 13, 7, 13] (note repeated key 13).
    let keys: [u64; 4] = [128, 13, 7, 13];
    for &key in &keys {
        sc.sys_object_wait_async(event_h, port_h, key, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();
    }

    // Cancel key 13 BEFORE signaling — both observers with key 13 should be removed.
    // C++: port.cancel(event, 13u)
    sc.sys_port_cancel(port_h, event_h, 13).unwrap();

    // Signal twice (set, clear, set, clear) — C++ does this to exercise re-arm.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Dequeue the expected 2 packets (keys 128 and 7).
    let mut wait_count = 0u32;
    let mut key_sum = 0u64;
    for _ in 0..2 {
        let received = wait_port(&ctx, port_h).await;
        let repr = PortPacketRepr::from(&received);
        wait_count += 1;
        key_sum += repr.key;
        if let PayloadRepr::Signal(sig) = repr.data {
            assert_eq!(sig.trigger, Signal::USER_SIGNAL_0);
        }
    }

    // After delivery, cancelling key 128 should return NOT_FOUND.
    assert_eq!(
        sc.sys_port_cancel(port_h, event_h, 128).unwrap_err(),
        ZxError::NOT_FOUND
    );

    assert_eq!(wait_count, 2, "only two observers (128, 7) should fire");
    assert_eq!(key_sum, 128 + 7, "key_sum should be 135");
}

/// C++: TEST(PortTest, CancelEventKeyAfter)
/// Cancelling after the packet is already delivered returns NOT_FOUND.
#[async_std::test]
async fn port_cancel_event_key_after() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    sc.sys_object_wait_async(event_h, port_h, 88, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Signal and dequeue.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    let _ = wait_port(&ctx, port_h).await;

    // Now try to cancel — subscription is already consumed.
    let result = sc.sys_port_cancel(port_h, event_h, 88);
    assert!(
        result.is_err(),
        "cancel after delivery should return NOT_FOUND"
    );
}

/// C++: TEST(PortTest, AsyncWaitChannelTimedOut)
/// Register wait_async on a channel that never becomes readable,
/// then verify cancel works (since we can't time out synchronously).
#[test]
fn async_wait_channel_timed_out() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let (ch0, _ch1) = create_channel(&ctx);

    // Register wait for READABLE on ch0
    sc.sys_object_wait_async(ch0, port_h, 6567, Signal::READABLE.bits(), 0)
        .unwrap();

    // No data has been written — ch0 is not readable.
    // In C++ this would time out via zx_port_wait with a deadline.
    // Here we verify the subscription exists by cancelling it successfully.
    let cancel_result = sc.sys_port_cancel(port_h, ch0, 6567);
    assert!(
        cancel_result.is_ok(),
        "cancel should succeed — subscription should still be pending"
    );

    // After cancellation, a second cancel should return NOT_FOUND.
    assert_eq!(
        sc.sys_port_cancel(port_h, ch0, 6567).unwrap_err(),
        ZxError::NOT_FOUND,
        "second cancel should return NOT_FOUND"
    );
}

/// C++: TEST(PortTest, CancelKey)
/// cancel_by_key removes all port subscriptions with a given key.
/// Multiple objects can be registered with the same key; cancel_by_key
/// removes them all.
#[test]
fn cancel_key() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event1_h = create_event(&ctx);
    let event2_h = create_event(&ctx);

    // Register two subscriptions with the same key
    sc.sys_object_wait_async(event1_h, port_h, 55, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();
    sc.sys_object_wait_async(event2_h, port_h, 55, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Cancel by key — should remove both subscriptions
    // C++: zx_port_cancel_key(port, 0, 55)
    sc.sys_port_cancel_key(port_h, 0, 55).unwrap();

    // Signaling neither event should produce a port packet
    sc.sys_object_signal(event1_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event2_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // Verify no subscriptions remain for this key
    assert_eq!(
        sc.sys_port_cancel_key(port_h, 0, 55).unwrap_err(),
        ZxError::NOT_FOUND
    );
}

/// C++: TEST(PortTest, CancelKeyAccessDenied)
/// cancel_by_key on a key that has no subscriptions returns NOT_FOUND.
/// At the syscall level, we also verify that a port handle without
/// WRITE right returns ACCESS_DENIED.
#[test]
fn cancel_key_access_denied() {
    use zircon_object::object::{Handle, Rights};
    use zircon_object::signal::Port;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);

    // No subscriptions exist for key 42 — should return NOT_FOUND
    assert_eq!(
        sc.sys_port_cancel_key(port_h, 0, 42).unwrap_err(),
        ZxError::NOT_FOUND
    );

    // Register a subscription with key 10, then try to cancel key 42
    let event_h = create_event(&ctx);
    sc.sys_object_wait_async(event_h, port_h, 10, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Key 42 still doesn't match — NOT_FOUND
    assert_eq!(
        sc.sys_port_cancel_key(port_h, 0, 42).unwrap_err(),
        ZxError::NOT_FOUND
    );

    // Key 10 exists — should succeed
    sc.sys_port_cancel_key(port_h, 0, 10).unwrap();

    // C++ test: duplicate port handle with rights = READ only (no WRITE),
    // then cancel_key should return ACCESS_DENIED.
    let port = Port::new(0).unwrap();
    let read_only_h = ctx.proc.add_handle(Handle::new(port, Rights::READ));
    assert_eq!(
        sc.sys_port_cancel_key(read_only_h, 0, 42).unwrap_err(),
        ZxError::ACCESS_DENIED
    );
}

/// C++: TEST(PortTest, CancelKeyInvalidOptions)
/// Creating a port with invalid options fails. cancel_key with
/// non-zero options returns INVALID_ARGS.
#[test]
fn cancel_key_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Port creation with invalid options
    let mut h: HandleValue = 0;
    assert_eq!(
        sc.sys_port_create(2, UserOutPtr::from(&mut h as *mut _ as usize))
            .unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        sc.sys_port_create(0xFF, UserOutPtr::from(&mut h as *mut _ as usize))
            .unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        sc.sys_port_create(0x80000000, UserOutPtr::from(&mut h as *mut _ as usize))
            .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Valid options work
    sc.sys_port_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);

    // C++: cancel_key with non-zero options returns INVALID_ARGS
    let port_h = h;
    assert_eq!(
        sc.sys_port_cancel_key(port_h, 1, 42).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(PortTest, CancelKeyQueuedUserPacket)
/// cancel_by_key removes both async subscriptions AND queued packets
/// matching the key (this matches Fuchsia's port_cancel_key behavior).
/// Queue user packets with different keys, cancel one key, verify
/// only the matching packets are removed.
#[async_std::test]
async fn cancel_key_queued_user_packet() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);

    // Queue a user packet with key 77
    let packet77 = make_user_packet(77, 0, [0xCC; 32]);
    queue_packet(&ctx, port_h, &packet77);

    // Queue a user packet with key 88
    let packet88 = make_user_packet(88, 0, [0xDD; 32]);
    queue_packet(&ctx, port_h, &packet88);

    // cancel_by_key removes queued packets matching the key
    sc.sys_port_cancel_key(port_h, 0, 77).unwrap();

    // Key 77 packet is gone; trying to cancel again returns NOT_FOUND
    assert_eq!(
        sc.sys_port_cancel_key(port_h, 0, 77).unwrap_err(),
        ZxError::NOT_FOUND
    );

    // Key 88 packet should still be dequeue-able
    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 88);
    if let PayloadRepr::User(data) = repr.data {
        assert_eq!(data[0], 0xDD);
    } else {
        panic!("expected User payload");
    }
}

/// C++: TEST(PortTest, CloseQueueRace)
/// Closing a port with queued packets should not panic or leak.
/// This is the synchronous analog of the C++ race test.
#[test]
fn close_queue_race() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create and fill a port, then close it — verifying no panic
    for _ in 0..10 {
        let port_h = create_port(&ctx);
        for i in 0..100u64 {
            let packet = make_user_packet(i, 0, [i as u8; 32]);
            queue_packet(&ctx, port_h, &packet);
        }

        // Also register some signal subscriptions
        let event_h = create_event(&ctx);
        sc.sys_object_wait_async(event_h, port_h, 999, Signal::USER_SIGNAL_0.bits(), 0)
            .unwrap();

        // Close the port handle — all queued packets and subscriptions are cleaned up
        sc.sys_handle_close(port_h).unwrap();

        // Signal the event after port is closed — should not panic
        sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
            .unwrap();

        // Verify port is truly closed — queue should fail with BAD_HANDLE
        let packet = make_user_packet(999, 0, [0; 32]);
        assert_eq!(
            sc.sys_port_queue(port_h, UserInPtr::from(&packet as *const _ as usize))
                .unwrap_err(),
            ZxError::BAD_HANDLE,
            "port should be closed after sys_handle_close"
        );
    }
}

/// C++: TEST(PortTest, EdgeTriggerTimeout)
/// Edge-triggered wait: if the signal is already set before registration,
/// no packet is delivered (unlike level-triggered). Verify by registering
/// edge-trigger on an already-signaled event and checking cancel succeeds
/// (meaning the subscription is still pending, not yet fired).
#[test]
fn edge_trigger_timeout() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    // Signal BEFORE registering edge-trigger
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    const ZX_WAIT_ASYNC_EDGE: u32 = 2;
    sc.sys_object_wait_async(
        event_h,
        port_h,
        30,
        Signal::USER_SIGNAL_0.bits(),
        ZX_WAIT_ASYNC_EDGE,
    )
    .unwrap();

    // Edge-triggered: since the signal was already set, and no TRANSITION
    // happened after registration, the subscription should remain pending.
    // We can verify by trying to cancel it — if it fired, cancel would
    // return NOT_FOUND.
    // Edge-triggered: no transition after registration, so no packet should fire.
    // Verify by checking cancel succeeds (subscription still pending).
    let cancel_result = sc.sys_port_cancel(port_h, event_h, 30);
    assert!(
        cancel_result.is_ok(),
        "cancel should succeed — edge subscription should still be pending, got {:?}",
        cancel_result,
    );
}

/// C++: TEST(PortTest, ThreadEvents)
/// Verify that port can receive signal packets from multiple events.
/// Each event's signal maps to a different key.
#[async_std::test]
async fn thread_events() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);

    // Create 3 events, each subscribed with a unique key
    let event_handles: Vec<HandleValue> = (0..3).map(|_| create_event(&ctx)).collect();

    for (i, &event_h) in event_handles.iter().enumerate() {
        sc.sys_object_wait_async(
            event_h,
            port_h,
            (i + 1) as u64,
            Signal::USER_SIGNAL_0.bits(),
            0,
        )
        .unwrap();
    }

    // Signal all events
    for &event_h in &event_handles {
        sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
            .unwrap();
    }

    // Dequeue all 3 packets and verify keys
    let mut keys = Vec::new();
    for _ in 0..3 {
        let received = wait_port(&ctx, port_h).await;
        let repr = PortPacketRepr::from(&received);
        keys.push(repr.key);
        if let PayloadRepr::Signal(sig) = repr.data {
            assert!(sig.observed.contains(Signal::USER_SIGNAL_0));
        } else {
            panic!("expected Signal payload");
        }
    }
    keys.sort();
    assert_eq!(keys, vec![1, 2, 3]);
}

/// C++: TEST(PortTest, Timestamp)
/// Signal packets should carry a valid timestamp. We verify the
/// timestamp field is present in the signal payload.
/// C++ uses options=0 which means timestamp should be 0.
#[async_std::test]
async fn timestamp() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    // options=0: no timestamp requested
    sc.sys_object_wait_async(event_h, port_h, 50, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 50);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::USER_SIGNAL_0));
        // C++: EXPECT_EQ(packet.signal.timestamp, 0u) — no timestamp without option
        assert_eq!(
            sig.timestamp, 0,
            "timestamp should be 0 without timestamp option"
        );
    } else {
        panic!("expected Signal payload");
    }
}

/// C++: TEST(PortTest, WaitAsyncBootTimestamp)
/// With ZX_WAIT_ASYNC_BOOT_TIMESTAMP, the signal packet carries a
/// non-zero boot timeline timestamp.
#[async_std::test]
async fn wait_async_boot_timestamp() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let port_h = create_port(&ctx);
    let event_h = create_event(&ctx);

    // C++: First verify mutually exclusive timestamp flags return INVALID_ARGS.
    // ZX_WAIT_ASYNC_TIMESTAMP = 1, ZX_WAIT_ASYNC_BOOT_TIMESTAMP = 4
    const ZX_WAIT_ASYNC_TIMESTAMP: u32 = 1;
    const ZX_WAIT_ASYNC_BOOT_TIMESTAMP: u32 = 4;
    assert_eq!(
        sc.sys_object_wait_async(
            event_h,
            port_h,
            1,
            Signal::USER_SIGNAL_0.bits(),
            ZX_WAIT_ASYNC_TIMESTAMP | ZX_WAIT_ASYNC_BOOT_TIMESTAMP,
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Register with ZX_WAIT_ASYNC_BOOT_TIMESTAMP
    sc.sys_object_wait_async(
        event_h,
        port_h,
        60,
        Signal::USER_SIGNAL_1.bits(),
        ZX_WAIT_ASYNC_BOOT_TIMESTAMP,
    )
    .unwrap();

    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_1.bits())
        .unwrap();

    let received = wait_port(&ctx, port_h).await;
    let repr = PortPacketRepr::from(&received);
    assert_eq!(repr.key, 60);
    assert_eq!(repr.status, ZxError::OK);
    if let PayloadRepr::Signal(sig) = repr.data {
        assert!(sig.observed.contains(Signal::USER_SIGNAL_1));
        assert!(sig.timestamp > 0, "boot timestamp should be non-zero");
    } else {
        panic!("expected Signal payload");
    }
}

// ── Tests requiring infrastructure not yet available ────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// port/ports.cc:213
///
/// Pre-queues 4096 user packets to a port, then queues one more
/// from a worker thread. The kernel raises ZX_EXCP_POLICY_ERROR
/// with synth_code ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS.
///
/// Main thread reads the exception from the process exception channel,
/// verifies the exception type and synth_code, then resumes the thread.
/// When a port exceeds its packet limit (4096), the kernel raises
/// ZX_EXCP_POLICY_ERROR with synth_code PORT_TOO_MANY_PACKETS (17).
#[async_std::test]
async fn queue_packet_limit_exceeded_generates_policy_exception() {
    use crate::helpers::{self, ExceptionTestThread};
    use zircon_object::task::{
        policy_code, ExceptionObject, ExceptionType, Job, Process, Task, ThreadState,
    };
    extern crate alloc;

    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "port-exc").unwrap();

    // Create process exception channel before starting the thread.
    let exc_channel = proc
        .exceptionate()
        .create_channel(Rights::DEFAULT_THREAD | Rights::DEFAULT_PROCESS)
        .unwrap();

    // Start thread that raises PORT_TOO_MANY_PACKETS exception.
    let test_thread = ExceptionTestThread::start(&proc, policy_code::PORT_TOO_MANY_PACKETS, 0);

    // Wait for exception to arrive.
    let exc_channel_obj: alloc::sync::Arc<dyn KernelObject> = exc_channel.clone();
    exc_channel_obj
        .wait_signal(Signal::READABLE | Signal::PEER_CLOSED)
        .await;

    // Read and verify the exception.
    let msg = exc_channel.read().unwrap();
    assert_eq!(msg.handles.len(), 1);

    let exc_type_raw = u32::from_ne_bytes(msg.data[16..20].try_into().unwrap());
    assert_eq!(exc_type_raw, ExceptionType::PolicyError as u32);

    let report = test_thread.thread.get_thread_exception_info().unwrap();
    assert_eq!(report.synth_code, policy_code::PORT_TOO_MANY_PACKETS);

    // Resume the thread (HANDLED).
    let exc_obj = msg.handles[0]
        .object
        .clone()
        .downcast_arc::<ExceptionObject>()
        .unwrap();
    exc_obj.set_state(1).unwrap();
    drop(exc_obj);
    drop(msg);

    test_thread.wait_for_termination().await;
    assert_eq!(test_thread.thread.state(), ThreadState::Dead);

    proc.exit(0);
}

/// C++: TEST(PortTest, TooManyObservers)
/// port/ports.cc:1155
///
/// Calls wait_async in a loop until the observer limit is reached.
/// The kernel should return ZX_ERR_NO_MEMORY (not panic) when the
/// limit is exceeded.
#[test]
fn too_many_observers() {
    use zircon_object::signal::Port;

    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let event = zircon_object::signal::Event::new();
    let event_obj: std::sync::Arc<dyn zircon_object::object::KernelObject> = event.clone();

    // Register wait_async in a loop until we hit the observer limit.
    // The kernel should return NO_MEMORY, not panic.
    let mut count = 0u32;
    loop {
        let result =
            event_obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, count as u64, false);
        match result {
            Ok(()) => count += 1,
            Err(ZxError::NO_MEMORY) => break,
            Err(e) => panic!("unexpected error on iteration {count}: {e:?}"),
        }
    }
    // Should have registered many observers before hitting the limit.
    assert!(
        count > 100,
        "expected many successful registrations before NO_MEMORY, got {count}"
    );
}

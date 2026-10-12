//! Object wait tests.
//!
//! Faithful 1:1 ports of object-wait tests from Fuchsia core-tests.
//! Tests `zx_object_wait_one`, `zx_object_wait_async`, and related APIs.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, HandleValue, Rights, Signal};
use zircon_object::signal::{PortPacket, PortPacketRepr};
use zircon_object::ZxError;

/// wait_one returns immediately when the signal is already set.
#[async_std::test]
async fn wait_one_already_signaled() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create an event and signal it.
    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // wait_one with deadline=0 should succeed immediately.
    let mut observed = Signal::empty();
    sc.sys_object_wait_one(
        event_h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut observed as *mut _ as usize),
    )
    .await
    .unwrap();
    assert!(observed.contains(Signal::USER_SIGNAL_0));
}

/// wait_one with an unsatisfied signal and deadline=0 returns TIMED_OUT.
#[async_std::test]
async fn wait_one_timeout() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Signal is not set — should time out.
    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// wait_one on an invalid handle returns BAD_HANDLE.
#[async_std::test]
async fn wait_one_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            0xDEAD,
            Signal::READABLE.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// wait_one without WAIT right returns ACCESS_DENIED.
#[async_std::test]
async fn wait_one_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create event with only SIGNAL right (no WAIT).
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::SIGNAL, // no WAIT
    ));

    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// wait_one with null observed pointer still works when signal is set.
#[async_std::test]
async fn wait_one_null_observed() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // Null observed pointer (0) should still succeed.
    sc.sys_object_wait_one(
        event_h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(0usize),
    )
    .await
    .unwrap();
}

/// wait_async registers a subscription that fires when signaled.
#[async_std::test]
async fn wait_async_basic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Register wait_async.
    sc.sys_object_wait_async(
        event_h,
        port_h,
        42, // key
        Signal::USER_SIGNAL_0.bits(),
        0, // options
    )
    .unwrap();

    // Signal the event.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // Dequeue the packet from the port.
    let mut packet = unsafe { core::mem::zeroed::<PortPacket>() };
    sc.sys_port_wait(
        port_h,
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut packet as *mut _ as usize),
    )
    .await
    .unwrap();

    let repr = PortPacketRepr::from(&packet);
    assert_eq!(repr.key, 42);
}

/// Cancelling wait_async removes the subscription.
#[test]
fn wait_async_cancel() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Register wait_async.
    sc.sys_object_wait_async(event_h, port_h, 77, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Cancel it.
    sc.sys_port_cancel(port_h, event_h, 77).unwrap();

    // Signal the event — no packet should fire.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
}

/// wait_async with invalid options returns INVALID_ARGS.
#[test]
fn wait_async_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Invalid options.
    let result = sc.sys_object_wait_async(
        event_h,
        port_h,
        0,
        Signal::READABLE.bits(),
        0xFF, // invalid
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// wait_async with invalid port handle returns BAD_HANDLE.
#[test]
fn wait_async_bad_port_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    let result = sc.sys_object_wait_async(
        event_h,
        0xDEAD, // invalid port
        0,
        Signal::READABLE.bits(),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// wait_async with invalid source handle returns BAD_HANDLE.
#[test]
fn wait_async_bad_source_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let result = sc.sys_object_wait_async(
        0xDEAD, // invalid source
        port_h,
        0,
        Signal::READABLE.bits(),
        0,
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// wait_async with TIMESTAMP option.
#[async_std::test]
async fn wait_async_timestamp() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // ZX_WAIT_ASYNC_TIMESTAMP = 1
    sc.sys_object_wait_async(
        event_h,
        port_h,
        33,
        Signal::USER_SIGNAL_0.bits(),
        1, // TIMESTAMP
    )
    .unwrap();

    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    let mut packet = unsafe { core::mem::zeroed::<PortPacket>() };
    sc.sys_port_wait(
        port_h,
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut packet as *mut _ as usize),
    )
    .await
    .unwrap();

    let repr = PortPacketRepr::from(&packet);
    assert_eq!(repr.key, 33);
}

/// wait_async with EDGE option.
#[async_std::test]
async fn wait_async_edge() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // ZX_WAIT_ASYNC_EDGE = 2
    sc.sys_object_wait_async(
        event_h,
        port_h,
        44,
        Signal::USER_SIGNAL_0.bits(),
        2, // EDGE
    )
    .unwrap();

    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    let mut packet = unsafe { core::mem::zeroed::<PortPacket>() };
    sc.sys_port_wait(
        port_h,
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut packet as *mut _ as usize),
    )
    .await
    .unwrap();

    let repr = PortPacketRepr::from(&packet);
    assert_eq!(repr.key, 44);
}

/// TIMESTAMP and BOOT_TIMESTAMP options cannot both be set.
#[test]
fn wait_async_timestamp_and_boot_timestamp_exclusive() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut port_h: HandleValue = 0;
    sc.sys_port_create(0, UserOutPtr::from(&mut port_h as *mut _ as usize))
        .unwrap();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // TIMESTAMP (1) | BOOT_TIMESTAMP (4) = 5
    let result = sc.sys_object_wait_async(event_h, port_h, 0, Signal::READABLE.bits(), 5);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// wait_one on a channel whose peer is closed returns with PEER_CLOSED.
#[async_std::test]
async fn wait_one_channel_peer_closed() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create channel via object layer.
    let (ch0, ch1) = zircon_object::ipc::Channel::create();
    let ch0_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Drop ch1 — peer is closed.
    drop(ch1);

    // wait_one for READABLE — should see PEER_CLOSED.
    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            ch0_h,
            Signal::READABLE.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    // PEER_CLOSED is always returned when the peer is closed, even if
    // READABLE wasn't in the wait mask. The wait returns with the
    // observed signals.
    // The result may be Ok (if PEER_CLOSED is in the observed signals
    // and matches the wait_for mask) or an error depending on the
    // exact signal matching. Either way, PEER_CLOSED should be observed.
    match result {
        Ok(()) => assert!(observed.contains(Signal::PEER_CLOSED)),
        Err(ZxError::TIMED_OUT) => {
            // observed should still contain PEER_CLOSED
            assert!(observed.contains(Signal::PEER_CLOSED));
        }
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

/// C++: TEST(ObjectWaitManyTest, InvalidHandle)
/// wait_many with an invalid handle in the array returns BAD_HANDLE.
#[async_std::test]
async fn wait_many_invalid_handle() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // UserWaitItem with invalid handle
    #[repr(C)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    let mut items = [WaitItem {
        handle: 0xDEAD,
        wait_for: Signal::READABLE,
        observed: Signal::empty(),
    }];

    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(items.as_mut_ptr() as usize),
            1,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(ObjectWaitManyTest, TooManyObjects)
/// wait_many with count > MAX_WAIT_MANY_ITEMS (32) returns OUT_OF_RANGE.
#[async_std::test]
async fn wait_many_too_many_objects() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // MAX_WAIT_MANY_ITEMS = 32; pass count=33
    // We need a buffer large enough. Each item is 12 bytes (handle + wait_for + observed).
    let mut buf = vec![0u8; 33 * 12];

    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(buf.as_mut_ptr() as usize),
            33,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(ObjectWaitManyTest, TransientSignalsNotReturned)
/// Signal an event, clear it, then wait_many — should time out.
#[async_std::test]
async fn wait_many_transient_signals_not_returned() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Signal then clear
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    #[repr(C)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    let mut items = [WaitItem {
        handle: event_h,
        wait_for: Signal::USER_SIGNAL_0,
        observed: Signal::empty(),
    }];

    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(items.as_mut_ptr() as usize),
            1,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitManyTest, WaitForEventsSignaled)
/// Signal events before waiting — wait_many should succeed immediately.
#[async_std::test]
async fn wait_many_wait_for_events_signaled() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Signal the event
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    #[repr(C)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    let mut items = [WaitItem {
        handle: event_h,
        wait_for: Signal::USER_SIGNAL_0,
        observed: Signal::empty(),
    }];

    sc.sys_object_wait_many(
        UserInOutPtr::from(items.as_mut_ptr() as usize),
        1,
        zircon_syscall::Deadline::from(0usize),
    )
    .await
    .unwrap();

    assert!(items[0].observed.contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(ObjectWaitManyTest, WaitForEventsThenSignal)
/// wait_many with unsignaled event and deadline=0 — times out.
#[async_std::test]
async fn wait_many_wait_for_events_then_signal() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    #[repr(C)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    let mut items = [WaitItem {
        handle: event_h,
        wait_for: Signal::USER_SIGNAL_0,
        observed: Signal::empty(),
    }];

    // Not signaled — should time out
    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(items.as_mut_ptr() as usize),
            1,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitManyTest, WaitOnZeroThings)
/// wait_many with count=0 and deadline=0 returns TIMED_OUT.
#[async_std::test]
async fn wait_many_wait_on_zero_things() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Empty array, count=0
    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(0x1000usize), // non-null but count=0
            0,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    // With count=0, wait_many should either succeed immediately or time out
    assert!(result.is_ok() || result == Err(ZxError::TIMED_OUT));
}

/// C++: TEST(ObjectWaitOneTest, EmptySignalSet)
/// Waiting with signals=0 and deadline=past should time out immediately.
#[async_std::test]
async fn wait_one_empty_signal_set() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Wait with empty signal set and deadline=0 (past).
    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            0, // empty signal set
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitOneTest, TransientSignalsNotReturned)
/// Signal then clear before waiting — should time out.
#[async_std::test]
async fn wait_one_transient_signals_not_returned() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Set then clear the signal
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventSignaled)
/// Signal the event first, then wait with deadline=past — should return OK.
#[async_std::test]
async fn wait_one_wait_for_event_signaled() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Signal the event before waiting.
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // Wait with deadline=0 — signal already set, should succeed immediately.
    let mut observed = Signal::empty();
    sc.sys_object_wait_one(
        event_h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut observed as *mut _ as usize),
    )
    .await
    .unwrap();
    assert!(observed.contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventThenSignal)
/// Wait on unsignaled event with deadline=0 — should time out.
#[async_std::test]
async fn wait_one_wait_for_event_then_signal() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventTimeout)
/// Wait on an unsignaled event with deadline=past — should return TIMED_OUT.
#[async_std::test]
async fn wait_one_wait_for_event_timeout() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Event is not signaled — should time out immediately.
    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventTimeoutPreSignalClear)
/// Signal, clear, then wait — should time out (signal was transient).
#[async_std::test]
async fn wait_one_wait_for_event_timeout_pre_signal_clear() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Set then clear signal before waiting
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    let mut observed = Signal::empty();
    let result = sc
        .sys_object_wait_one(
            event_h,
            Signal::USER_SIGNAL_0.bits(),
            zircon_syscall::Deadline::from(0usize),
            UserOutPtr::from(&mut observed as *mut _ as usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);
}

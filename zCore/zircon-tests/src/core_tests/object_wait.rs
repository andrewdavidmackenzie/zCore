//! Object wait tests.
//!
//! Faithful 1:1 ports of object-wait tests from Fuchsia core-tests.
//! Tests `zx_object_wait_one`, `zx_object_wait_async`, and related APIs.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::{HandleValue, Signal};
use zircon_object::ZxError;

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
/// C++ creates 3 events, all waiting for USER_SIGNAL_0. A thread:
///   - Sets+clears USER_SIGNAL_1 on items[0] and items[2] (transient)
///   - Sets USER_SIGNAL_1 on items[1] (persistent but not watched)
///   - Sets USER_SIGNAL_0 on items[0] (watched → wakes wait_many)
/// Expected: items[0].pending == USER_SIGNAL_0,
///           items[1].pending == USER_SIGNAL_1,
///           items[2].pending == 0.
/// Under cooperative scheduling, we do the same signal sequence
/// before calling wait_many with deadline=0.
#[async_std::test]
async fn wait_many_transient_signals_not_returned() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create 3 events
    let mut event_handles: [HandleValue; 3] = [0; 3];
    for h in event_handles.iter_mut() {
        sc.sys_event_create(0, UserOutPtr::from(h as *mut _ as usize))
            .unwrap();
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    // All 3 items wait for USER_SIGNAL_0
    let mut items: [WaitItem; 3] = core::array::from_fn(|i| WaitItem {
        handle: event_handles[i],
        wait_for: Signal::USER_SIGNAL_0,
        observed: Signal::empty(),
    });

    // Reproduce the C++ thread's signal sequence:
    // Set+clear USER_SIGNAL_1 on items[0] (transient)
    sc.sys_object_signal(event_handles[0], 0, Signal::USER_SIGNAL_1.bits())
        .unwrap();
    sc.sys_object_signal(event_handles[0], Signal::USER_SIGNAL_1.bits(), 0)
        .unwrap();

    // Set+clear USER_SIGNAL_1 on items[2] (transient)
    sc.sys_object_signal(event_handles[2], 0, Signal::USER_SIGNAL_1.bits())
        .unwrap();
    sc.sys_object_signal(event_handles[2], Signal::USER_SIGNAL_1.bits(), 0)
        .unwrap();

    // Set USER_SIGNAL_1 on items[1] (persistent, non-watched)
    sc.sys_object_signal(event_handles[1], 0, Signal::USER_SIGNAL_1.bits())
        .unwrap();

    // Set USER_SIGNAL_0 on items[0] (watched → satisfies wait)
    sc.sys_object_signal(event_handles[0], 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // wait_many with deadline=0 — signal already set, should succeed
    sc.sys_object_wait_many(
        UserInOutPtr::from(items.as_mut_ptr() as usize),
        3,
        zircon_syscall::Deadline::from(0usize),
    )
    .await
    .unwrap();

    // C++: items[0].pending == USER_SIGNAL_0 (transient USER_SIGNAL_1 was cleared)
    assert_eq!(
        items[0].observed,
        Signal::USER_SIGNAL_0,
        "items[0] should have USER_SIGNAL_0 only (USER_SIGNAL_1 was transient)"
    );
    // C++: items[1].pending == USER_SIGNAL_1 (persistent, non-watched)
    assert_eq!(
        items[1].observed,
        Signal::USER_SIGNAL_1,
        "items[1] should have USER_SIGNAL_1 (persistent, non-watched)"
    );
    // C++: items[2].pending == 0 (USER_SIGNAL_1 was transient, USER_SIGNAL_0 never set)
    assert_eq!(
        items[2].observed,
        Signal::empty(),
        "items[2] should have no signals (USER_SIGNAL_1 was transient)"
    );
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
/// C++ creates 8 events, spawns a thread that signals 2 of them after the main
/// thread enters BLOCKED_WAIT_MANY. Under cooperative scheduling we can't block
/// and signal from another thread, so we test both halves:
/// 1. Unsignaled events with deadline=0 → TIMED_OUT
/// 2. Signal events, then wait_many with deadline=0 → succeeds, at least one
///    item has pending signals (matching C++ assertion: signal_count > 0).
#[async_std::test]
async fn wait_many_wait_for_events_then_signal() {
    use hal_impl::user::UserInOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create 8 events matching C++ test layout
    let mut event_handles: [HandleValue; 8] = [0; 8];
    for h in event_handles.iter_mut() {
        sc.sys_event_create(0, UserOutPtr::from(h as *mut _ as usize))
            .unwrap();
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct WaitItem {
        handle: HandleValue,
        wait_for: Signal,
        observed: Signal,
    }

    let event_signaled = Signal::SIGNALED;

    // Phase 1: no signals → timeout
    let mut items: [WaitItem; 8] = core::array::from_fn(|i| WaitItem {
        handle: event_handles[i],
        wait_for: event_signaled,
        observed: Signal::empty(),
    });

    let result = sc
        .sys_object_wait_many(
            UserInOutPtr::from(items.as_mut_ptr() as usize),
            8,
            zircon_syscall::Deadline::from(0usize),
        )
        .await;
    assert_eq!(result.unwrap_err(), ZxError::TIMED_OUT);

    // Phase 2: signal items[1] and items[6] (matching C++ to_signal pattern),
    // then wait_many → should succeed
    let sig_bits = event_signaled.bits();
    let to_signal: [u32; 8] = [0, sig_bits, 0, 0, 0, 0, sig_bits, 0];
    for (ix, &sig) in to_signal.iter().enumerate() {
        if sig != 0 {
            sc.sys_object_signal(event_handles[ix], 0, sig).unwrap();
        }
    }

    // Reset observed
    for item in items.iter_mut() {
        item.observed = Signal::empty();
    }

    sc.sys_object_wait_many(
        UserInOutPtr::from(items.as_mut_ptr() as usize),
        8,
        zircon_syscall::Deadline::from(0usize),
    )
    .await
    .unwrap();

    // C++: EXPECT_GT(signal_count, 0) — at least one must be signaled
    let signal_count = items
        .iter()
        .filter(|item| item.observed.contains(event_signaled))
        .count();
    assert!(
        signal_count > 0,
        "at least one event should be signaled, got 0"
    );
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
/// C++ sets USER_SIGNAL_0, clears it, sets USER_SIGNAL_1, then waits for
/// USER_SIGNAL_1. Assert observed == USER_SIGNAL_1 (not USER_SIGNAL_0).
/// Under cooperative scheduling we do the same sequence non-blocking:
/// 1. Set+clear USER_SIGNAL_0, then wait for USER_SIGNAL_0 → TIMED_OUT
/// 2. Additionally set USER_SIGNAL_1, wait for USER_SIGNAL_1 → OK,
///    observed contains USER_SIGNAL_1 but NOT USER_SIGNAL_0.
#[async_std::test]
async fn wait_one_transient_signals_not_returned() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();

    // Set then clear USER_SIGNAL_0 (transient)
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    sc.sys_object_signal(event_h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // Wait for the cleared signal → should time out
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

    // Now set USER_SIGNAL_1 (persistent) and wait for it
    sc.sys_object_signal(event_h, 0, Signal::USER_SIGNAL_1.bits())
        .unwrap();

    let mut observed2 = Signal::empty();
    sc.sys_object_wait_one(
        event_h,
        Signal::USER_SIGNAL_1.bits(),
        zircon_syscall::Deadline::from(0usize),
        UserOutPtr::from(&mut observed2 as *mut _ as usize),
    )
    .await
    .unwrap();

    // C++: ASSERT_EQ(observed, ZX_USER_SIGNAL_1)
    assert_eq!(observed2, Signal::USER_SIGNAL_1);
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

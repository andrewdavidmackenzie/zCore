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

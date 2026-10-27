//! Object wait tests.
//!
//! Faithful 1:1 ports of object-wait tests from Fuchsia core-tests.
//! Tests `zx_object_wait_one`, `zx_object_wait_async`, and related APIs.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::{HandleValue, Signal};
use zircon_object::ZxError;
/// C++: TEST(ObjectWaitManyTest, TransientSignalsNotReturned)
/// C++ creates 3 events, all waiting for USER_SIGNAL_0. A thread:
///   - Sets+clears USER_SIGNAL_1 on items[0] and items[2] (transient)
///   - Sets USER_SIGNAL_1 on items[1] (persistent but not watched)
///   - Sets USER_SIGNAL_0 on items[0] (watched → wakes wait_many)
/// Expected: items[0].pending == USER_SIGNAL_0,
///           items[1].pending == USER_SIGNAL_1,
///           items[2].pending == 0.
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
/// C++: TEST(ObjectWaitOneTest, TransientSignalsNotReturned)
/// C++ sets USER_SIGNAL_0, clears it, sets USER_SIGNAL_1, then waits for
/// USER_SIGNAL_1. Assert observed == USER_SIGNAL_1 (not USER_SIGNAL_0).
/// TODO review this since pre-emptive scheduling
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

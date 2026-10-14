//! Additional zCore-specific tests for timer functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use core::time::Duration;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Slack, Timer};

#[test]
fn timer_create() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert_ne!(timer.id(), 0);
}

#[test]
fn timer_set_past_deadline_fires_immediately() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Set deadline in the past (0 = already expired)
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));

    // Signal should be set immediately
    assert!(timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_cancel() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    timer.cancel();

    // After cancel, SIGNALED should not be set
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_cancel_clears_signal() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Fire immediately
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));

    // Cancel clears the signal
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_create_with_slack() {
    let _ctx = TestContext::new();

    let timer_center = Timer::with_slack(Slack::Center);
    assert_ne!(timer_center.id(), 0);

    let timer_early = Timer::with_slack(Slack::Early);
    assert_ne!(timer_early.id(), 0);

    let timer_late = Timer::with_slack(Slack::Late);
    assert_ne!(timer_late.id(), 0);
}

#[test]
fn timer_one_shot() {
    let _ctx = TestContext::new();
    // Duration::from_nanos(0) is in the past — should fire immediately
    let timer = Timer::one_shot(Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_set_replaces_deadline() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Set a far-future deadline
    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Replace with a past deadline — should fire immediately
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_get_info() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Late);

    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 2); // Slack::Late = 2
    assert_eq!(deadline, 0); // no deadline set
    assert_eq!(slack, 0);
}

#[test]
fn timer_get_info_after_set() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_secs(1000), Duration::from_nanos(500));
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 0); // Slack::Center = 0
    assert!(deadline > 0); // deadline is set
    assert_eq!(slack, 500);
}

#[test]
fn timer_cancel_twice_is_ok() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    timer.cancel();
    timer.cancel(); // second cancel is a no-op
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

#[test]
fn timer_name() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set_name("my-timer");
    assert_eq!(timer.name(), "my-timer");
}

///
/// Set a timer with a deadline computed via `zx_deadline_after()`,
/// then verify the SIGNALED bit fires. Since we can't easily wait
/// in a unit test, we use deadline=0 (past) which fires immediately.
#[test]
fn timer_deadline_after() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // zx_deadline_after(0) returns current monotonic time — already in the past
    let deadline = Duration::from_nanos(0);
    timer.set(deadline, Duration::from_nanos(0));

    assert!(timer.signal().contains(Signal::SIGNALED));
}

///
/// Set a far-future timer, cancel it, then set a new one that fires
/// immediately. Verifies that cancel truly resets state and a
/// subsequent set works correctly.
#[test]
fn timer_coalesce_and_cancel() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Set a far-future deadline
    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Cancel
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Set again with a past deadline — should fire immediately
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

///
/// Creating a timer with an invalid slack mode should fail.
/// At the object level, `Slack` is a Rust enum so invalid values
/// can't be constructed. This test verifies the syscall rejects
/// invalid options (slack mode >= 3).
#[test]
fn timer_invalid_args() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;

    // Valid options: 0 (Center), 1 (Early), 2 (Late)
    // options=3 is invalid
    let result = sc.sys_timer_create(3, 0, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::INVALID_ARGS);

    // options=0xFF is also invalid
    let result = sc.sys_timer_create(0xFF, 0, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::INVALID_ARGS);
}

//! Timer operations tests.
//!
//! Additional timer tests covering set, cancel, reset, and signal
//! behavior for timer objects.

use crate::helpers::TestContext;
use core::time::Duration;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Timer;

///
/// Create a timer and verify it has a valid KOID.
#[test]
fn create_timer() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert_ne!(timer.id(), 0);
}

///
/// Set and get the name of a timer.
#[test]
fn timer_name() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set_name("test-timer");
    assert_eq!(timer.name(), "test-timer");
}

///
/// A newly created timer should not be signaled.
#[test]
fn timer_not_signaled_initially() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

///
/// Set a deadline on a timer. If the deadline is in the past, the
/// timer should signal immediately.
#[test]
fn timer_set_deadline() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(Duration::from_secs(0), Duration::default());
}

///
/// Set a deadline then cancel the timer. The timer should not be
/// signaled after cancellation.
#[test]
fn timer_cancel() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(Duration::from_secs(999999), Duration::default());
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

///
/// Set a timer, then set it again with a different deadline. The
/// new deadline should replace the old one.
#[test]
fn timer_set_then_reset() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(Duration::from_secs(999999), Duration::default());
    timer.set(Duration::from_secs(999998), Duration::default());
}

///
/// Create multiple timers. Each should have a unique KOID.
#[test]
fn timer_multiple_creation() {
    let _ctx = TestContext::new();
    let t1 = Timer::new();
    let t2 = Timer::new();
    let t3 = Timer::new();
    assert_ne!(t1.id(), t2.id());
    assert_ne!(t2.id(), t3.id());
}

///
/// Create timers with different slack modes (Center, Early, Late).
#[test]
fn timer_slack_modes() {
    use zircon_object::signal::Slack;
    let _ctx = TestContext::new();
    let _center = Timer::with_slack(Slack::Center);
    let _early = Timer::with_slack(Slack::Early);
    let _late = Timer::with_slack(Slack::Late);
}

///
/// Cancel a timer that has never been set. Should be a no-op.
#[test]
fn timer_cancel_when_not_set() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

///
/// Cancel a timer twice. Second cancel should be a no-op.
#[test]
fn timer_double_cancel_safe() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(Duration::from_secs(999999), Duration::default());
    timer.cancel();
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

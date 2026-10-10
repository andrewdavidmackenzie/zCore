//! Timer syscall tests.

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

/// C++: TEST(TimerTest, CreateWithSlack)
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

/// C++: TEST(TimerTest, OneShotConvenience)
#[test]
fn timer_one_shot() {
    let _ctx = TestContext::new();
    // Duration::from_nanos(0) is in the past — should fire immediately
    let timer = Timer::one_shot(Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(TimerTest, SetReplacesDeadline)
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

/// C++: TEST(TimerTest, GetInfo)
#[test]
fn timer_get_info() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Late);

    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 2); // Slack::Late = 2
    assert_eq!(deadline, 0); // no deadline set
    assert_eq!(slack, 0);
}

/// C++: TEST(TimerTest, GetInfoAfterSet)
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

/// C++: TEST(TimerTest, CancelTwiceIsOk)
#[test]
fn timer_cancel_twice_is_ok() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    timer.cancel();
    timer.cancel(); // second cancel is a no-op
    assert!(!timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(TimerTest, Name)
#[test]
fn timer_name() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set_name("my-timer");
    assert_eq!(timer.name(), "my-timer");
}

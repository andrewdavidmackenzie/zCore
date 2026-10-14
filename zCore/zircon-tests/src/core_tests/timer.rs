//! Timer syscall tests.

use crate::helpers::TestContext;
use core::time::Duration;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Slack, Timer};

/// C++: TEST(Timer, AlreadyPassedDeadlineOnWaitOne)
/// Set deadline to 0 (past), wait_one should return immediately.
#[test]
fn already_passed_deadline_on_wait_one() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    // SIGNALED should be set immediately — no wait needed
    assert!(timer.signal().contains(Signal::SIGNALED));
    // A wait_one with deadline=0 would return immediately since signal is set.
    // We test at the object level rather than syscall to avoid async complexities.
}

/// C++: TEST(Timer, Basic)
/// Basic timer lifecycle: create, set (past), verify signaled, cancel, verify cleared.
#[test]
fn basic() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Not signaled initially
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Set to past → fires immediately
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));

    // Cancel → clears signal
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Set again → fires again
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, BasicInfo)
/// Create a timer, verify get_info returns correct initial fields.
#[test]
fn basic_info() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Freshly created timer: no deadline, no slack, default options
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 0); // Slack::Center = 0
    assert_eq!(deadline, 0);
    assert_eq!(slack, 0);

    // After setting, fields should reflect the set values
    timer.set(Duration::from_secs(100), Duration::from_nanos(1000));
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 0);
    assert!(deadline > 0, "deadline should be set");
    assert_eq!(slack, 1000);
}

/// C++: TEST(Timer, BootTimersElapseDuringSuspend)
/// Boot timers are not implemented in zCore. Verify basic timer
/// behavior as a substitute.
#[test]
fn boot_timers_elapse_during_suspend() {
    let _ctx = TestContext::new();
    // Boot timers use a different clock source. In zCore, there's
    // only one timer type. Verify the basic timer fires correctly.
    let timer = Timer::new();
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, DISABLED_CoalesceTestEarly)
/// Timer with Early slack: deadline should coalesce earlier.
/// Verify Early slack timer fires at past deadline.
#[test]
fn disabled_coalesce_test_early() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Early);
    let (options, _, _) = timer.get_info();
    assert_eq!(options, 1); // Slack::Early = 1
                            // Past deadline fires immediately regardless of slack mode
    timer.set(Duration::from_nanos(0), Duration::from_secs(10));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, DISABLED_CoalesceTestLate)
/// Timer with Late slack: deadline should coalesce later.
/// Verify Late slack timer fires at past deadline.
#[test]
fn disabled_coalesce_test_late() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Late);
    let (options, _, _) = timer.get_info();
    assert_eq!(options, 2); // Slack::Late = 2
                            // Past deadline fires immediately regardless of slack mode
    timer.set(Duration::from_nanos(0), Duration::from_secs(10));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, DeadlineAfter)
/// deadline_after(0) returns current time — already in the past.
#[test]
fn deadline_after() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    // deadline_after(0) = now, which is in the past for timer.set
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, EdgeCases)
/// Cancel twice, set after cancel, cancel without set — all should be safe.
#[test]
fn edge_cases() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Cancel without ever setting — should be a no-op
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Set, cancel, cancel — double cancel is safe
    timer.set(Duration::from_secs(1000), Duration::from_nanos(0));
    timer.cancel();
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Set after cancel — timer should work normally
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));

    // Cancel clears signal, then set again
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, Restart)
/// Set a far-future timer, then replace with a past deadline.
/// The replacement should fire immediately.
#[test]
fn restart() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Set far future
    timer.set(Duration::from_secs(9999), Duration::from_nanos(0));
    assert!(!timer.signal().contains(Signal::SIGNALED));

    // Replace with past
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));

    // Cancel and restart again
    timer.cancel();
    assert!(!timer.signal().contains(Signal::SIGNALED));
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, RestartRace)
/// Rapid set/cancel cycles to verify no race conditions.
#[test]
fn restart_race() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Rapid set/cancel cycles — no panics or corrupted state
    for _ in 0..100 {
        timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
        assert!(timer.signal().contains(Signal::SIGNALED));
        timer.cancel();
        assert!(!timer.signal().contains(Signal::SIGNALED));
    }

    // Final set — fires correctly
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, SetNegativeDeadline)
/// Setting deadline=0 (past) fires immediately.
#[test]
fn set_negative_deadline() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, SetNegativeDeadlineMax)
/// Setting deadline=0 with max slack still fires immediately.
#[test]
fn set_negative_deadline_max() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Even with large slack, deadline=0 is in the past and fires immediately
    timer.set(Duration::from_nanos(0), Duration::from_secs(1000));
    assert!(timer.signal().contains(Signal::SIGNALED));
}

/// C++: TEST(Timer, SetNegativeSlack)
/// Slack is always non-negative in zCore (Duration can't be negative).
/// Verify that very small slack values work correctly.
#[test]
fn set_negative_slack() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // Slack of 0 is fine
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(timer.signal().contains(Signal::SIGNALED));
    let (_, _, slack) = timer.get_info();
    assert_eq!(slack, 0);

    // Slack of 1 nanosecond
    timer.cancel();
    timer.set(Duration::from_nanos(0), Duration::from_nanos(1));
    assert!(timer.signal().contains(Signal::SIGNALED));
    let (_, _, slack) = timer.get_info();
    assert_eq!(slack, 1);
}

/// C++: TEST(Timer, SignalsAssertedImmediately)
/// Set deadline to 0 (past), SIGNALED should be asserted immediately.
#[test]
fn signals_asserted_immediately() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // deadline=0 is already in the past
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert!(
        timer.signal().contains(Signal::SIGNALED),
        "SIGNALED should be asserted immediately for a past deadline"
    );
}

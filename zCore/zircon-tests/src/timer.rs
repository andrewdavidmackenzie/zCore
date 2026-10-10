//! Timer syscall tests.

use crate::helpers::TestContext;
use core::time::Duration;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Timer;

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

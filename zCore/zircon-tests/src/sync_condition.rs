//! Sync condition tests.
//!
//! Ports of Fuchsia's sync condition test suite. Tests the
//! kernel sync_condition primitive.
//! zCore has no sync_condition_variable. These tests verify
//! the underlying signal-based waiting that condition variables use.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Event;

extern crate alloc;

/// C++: TEST(SyncCondition, ConditionTest)
/// Simulate condition variable signal-and-check using Event + signals.
#[test]
fn condition_test() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // "Condition not met" — no signal set.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));

    // "Signal the condition."
    event.signal_set(Signal::USER_SIGNAL_0);

    // "Condition is met" — signal is set.
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // "Reset the condition."
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(SyncCondition, TimeoutTest)
/// Verify that a non-signaled event can be checked without blocking.
/// The actual timeout behavior requires multi-threaded wait, so we
/// verify the poll-style check (signal not set = would timeout).
#[test]
fn timeout_test() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Event is not signaled — a wait would timeout.
    let sig = event.signal();
    assert!(
        !sig.contains(Signal::USER_SIGNAL_0),
        "unsignaled event should not contain USER_SIGNAL_0"
    );

    // Signal, then verify immediate availability (no timeout).
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(
        event.signal().contains(Signal::USER_SIGNAL_0),
        "signaled event should be immediately available"
    );
}

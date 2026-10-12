//! Completion wrapper tests.
//!
//! Ports of Fuchsia's completion wrapper test suite. Tests the
//! C++ completion wrapper around sync_completion.
//! zCore has no completion wrapper. These tests verify the
//! underlying Event + Signal mechanism that completions use.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Event;

extern crate alloc;

/// C++: TEST(CompletionWrapper, Wait)
/// Simulate completion wait: check signal on an event.
#[test]
fn wait() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Not signaled — wait would block.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));

    // Signal the completion.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Now wait would return immediately.
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(CompletionWrapper, WaitDeadline)
/// Check that a signaled event is immediately available.
#[test]
fn wait_deadline() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(CompletionWrapper, WaitDeadlineTimeout)
/// An unsignaled event represents a timeout condition.
#[test]
fn wait_deadline_timeout() {
    let _ctx = TestContext::new();
    let event = Event::new();
    // Not signaled = would timeout.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(CompletionWrapper, WaitDuration)
/// Signal and verify immediate availability.
#[test]
fn wait_duration() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(CompletionWrapper, WaitDurationTimeout)
/// Unsignaled event = timeout.
#[test]
fn wait_duration_timeout() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(CompletionWrapper, signaled)
/// Signal, check, clear, check cycle.
#[test]
fn signaled() {
    let _ctx = TestContext::new();
    let event = Event::new();

    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

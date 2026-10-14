//! Pthread barrier tests.
//!
//! Ports of Fuchsia's pthread barrier test suite. Tests
//! pthread_barrier operations.
//! zCore has no pthread barrier infrastructure. These tests verify
//! the underlying signal-based synchronization mechanisms.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Event;

extern crate alloc;

/// C++: TEST(PThreadBarrierTest, InitWithNoThreadsReturnsInval)
/// A barrier with count=0 is invalid. Simulate by verifying that
/// an event with no signal set represents an "uninitialized" barrier.
#[test]
fn init_with_no_threads_returns_inval() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // No signals set = barrier not initialized/triggered.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));

    // Attempting to "wait" (check) on an unsignaled barrier finds nothing.
    let sig = event.signal();
    assert!(
        !sig.contains(Signal::USER_SIGNAL_0),
        "uninitialized barrier should have no signal"
    );
}

/// C++: TEST(PThreadBarrierTest, SingleThreadWinsBarrierObject)
/// A single thread hitting a barrier with count=1 immediately wins.
/// Simulate by signaling and checking.
#[test]
fn single_thread_wins_barrier_object() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Thread hits the barrier (count=1, so it immediately signals).
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(
        event.signal().contains(Signal::USER_SIGNAL_0),
        "single-thread barrier should be immediately satisfied"
    );
}

/// C++: TEST(PThreadBarrierTest, SingleThreadWinsBarrierObjectResetsBetweenIterations)
/// A barrier resets after being satisfied. Simulate with signal/clear cycle.
#[test]
fn single_thread_wins_barrier_object_resets_between_iterations() {
    let _ctx = TestContext::new();
    let event = Event::new();

    for _ in 0..5 {
        // Thread hits the barrier.
        event.signal_set(Signal::USER_SIGNAL_0);
        assert!(event.signal().contains(Signal::USER_SIGNAL_0));

        // Barrier resets.
        event.signal_clear(Signal::USER_SIGNAL_0);
        assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    }
}

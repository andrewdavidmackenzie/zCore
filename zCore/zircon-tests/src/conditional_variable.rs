//! Conditional variable tests.
//!
//! Ports of Fuchsia's conditional variable test suite. Tests
//! C11 condition variable operations.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ConditionalVariableTest, BroadcastSignalThreadWait)
#[test]
#[ignore = "requires condition variable with threads"]
fn broadcast_signal_thread_wait() {
    todo!("requires condition variable with threads");
}

/// C++: TEST(ConditionalVariableTest, ConditionalVariablesTimeout)
#[test]
#[ignore = "requires condition variable with threads"]
fn conditional_variables_timeout() {
    todo!("requires condition variable with threads");
}

/// C++: TEST(ConditionalVariableTest, SignalThreadWait)
#[test]
#[ignore = "requires condition variable with threads"]
fn signal_thread_wait() {
    todo!("requires condition variable with threads");
}

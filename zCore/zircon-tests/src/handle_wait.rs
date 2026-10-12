//! Handle wait tests.
//!
//! Ports of Fuchsia's handle wait test suite (HandleWaitTest).
//! Tests waiting on handle events and multi-threaded waiting.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(HandleWaitTest, HandleWaitTest)
#[test]
#[ignore = "requires multi-handle wait infrastructure"]
fn handle_wait_test() {
    todo!("requires multi-handle wait infrastructure");
}

/// C++: TEST(HandleWaitTest, HandleWaitMultipleThreads)
#[test]
#[ignore = "requires multi-threaded handle wait"]
fn handle_wait_multiple_threads() {
    todo!("requires multi-threaded handle wait");
}

//! Stack tests.
//!
//! Ports of Fuchsia's stack test suite. Tests stack address
//! and size queries for main and spawned threads.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(StackTest, MainThreadStack)
#[test]
#[ignore = "requires stack introspection infrastructure"]
fn main_thread_stack() {
    todo!("requires stack introspection infrastructure");
}

/// C++: TEST(StackTest, ThreadStack)
#[test]
#[ignore = "requires stack introspection infrastructure"]
fn thread_stack() {
    todo!("requires stack introspection infrastructure");
}

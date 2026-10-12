//! Pthread barrier tests.
//!
//! Ports of Fuchsia's pthread barrier test suite. Tests
//! pthread_barrier operations.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(PThreadBarrierTest, InitWithNoThreadsReturnsInval)
#[test]
#[ignore = "requires pthread barrier with threads"]
fn init_with_no_threads_returns_inval() {
    todo!("requires pthread barrier with threads");
}

/// C++: TEST(PThreadBarrierTest, SingleThreadWinsBarrierObject)
#[test]
#[ignore = "requires pthread barrier with threads"]
fn single_thread_wins_barrier_object() {
    todo!("requires pthread barrier with threads");
}

/// C++: TEST(PThreadBarrierTest, SingleThreadWinsBarrierObjectResetsBetweenIterations)
#[test]
#[ignore = "requires pthread barrier with threads"]
fn single_thread_wins_barrier_object_resets_between_iterations() {
    todo!("requires pthread barrier with threads");
}

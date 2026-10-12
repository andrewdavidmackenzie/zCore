//! Sync condition tests.
//!
//! Ports of Fuchsia's sync condition test suite. Tests the
//! kernel sync_condition primitive.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(SyncCondition, ConditionTest)
#[test]
#[ignore = "requires sync condition variable with threads"]
fn condition_test() {
    todo!("requires sync condition variable with threads");
}

/// C++: TEST(SyncCondition, TimeoutTest)
#[test]
#[ignore = "requires sync condition variable with threads"]
fn timeout_test() {
    todo!("requires sync condition variable with threads");
}

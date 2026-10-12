//! User exception tests.
//!
//! Ports of Fuchsia's user exception test suite. Tests
//! user-mode exception delivery and handling.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(UserExceptionsTest, Delivered)
#[test]
#[ignore = "requires user exception handling"]
fn delivered() {
    todo!("requires user exception handling");
}

/// C++: TEST(UserExceptionsTest, InvalidArgs)
#[test]
#[ignore = "requires user exception handling"]
fn invalid_args() {
    todo!("requires user exception handling");
}

/// C++: TEST(UserExceptionsTest, NonFatal)
#[test]
#[ignore = "requires user exception handling"]
fn non_fatal() {
    todo!("requires user exception handling");
}

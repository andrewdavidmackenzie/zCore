//! Interrupt crash tests.
//!
//! Ports of Fuchsia's interrupt crash test suite. Tests
//! that interrupt-related crashes are handled correctly.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(TestInterrupt, TestIntCrashes)
#[test]
#[ignore = "requires interrupt crash testing"]
fn test_int_crashes() {
    todo!("requires interrupt crash testing");
}

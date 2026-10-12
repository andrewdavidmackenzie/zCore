//! Ticks tests.
//!
//! Ports of Fuchsia's ticks test suite. Tests monotonic tick
//! counter behavior and time value representations.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(TicksTest, ElapsedTimeUsingTicks)
#[test]
#[ignore = "requires ticks query infrastructure"]
fn elapsed_time_using_ticks() {
    todo!("requires ticks query infrastructure");
}

/// C++: TEST(TicksTest, TimeValuesPaddingLeak)
#[test]
#[ignore = "requires ticks query infrastructure"]
fn time_values_padding_leak() {
    todo!("requires ticks query infrastructure");
}

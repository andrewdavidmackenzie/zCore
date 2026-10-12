//! Register context switch tests.
//!
//! Ports of Fuchsia's register context switch test suite. Tests
//! that register state is properly saved/restored across context switches.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(RegisterContextSwitchTests, FpuLongComputeLoop)
#[test]
#[ignore = "requires hardware context switch testing"]
fn fpu_long_compute_loop() {
    todo!("requires hardware context switch testing");
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnContextSwitch)
#[test]
#[ignore = "requires hardware context switch testing"]
fn segment_selectors_zeroed_on_context_switch() {
    todo!("requires hardware context switch testing");
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnInterrupt)
#[test]
#[ignore = "requires hardware context switch testing"]
fn segment_selectors_zeroed_on_interrupt() {
    todo!("requires hardware context switch testing");
}

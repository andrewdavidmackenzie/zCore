//! CPU feature tests.
//!
//! Ports of Fuchsia's CPU feature test suite. Tests detection
//! and use of CPU-specific features like ARM MOPS.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(CpuFeatureTests, ArmMops)
#[test]
#[ignore = "requires CPU feature detection hardware"]
fn arm_mops() {
    todo!("requires CPU feature detection hardware");
}

/// C++: TEST(CpuFeatureTests, ArmMopsException)
#[test]
#[ignore = "requires CPU feature detection hardware"]
fn arm_mops_exception() {
    todo!("requires CPU feature detection hardware");
}

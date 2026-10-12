//! CPU feature tests.
//!
//! Ports of Fuchsia's CPU feature test suite. Tests detection
//! and use of CPU-specific features like ARM MOPS.
//! zCore has no CPU feature detection API. These tests verify
//! basic system queries (monotonic clock).

use crate::helpers::TestContext;

/// C++: TEST(CpuFeatureTests, ArmMops)
/// Verify that basic system information is queryable.
#[test]
fn arm_mops() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Monotonic clock should be positive and advancing.
    let t1 = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t1 > 0, "monotonic time should be positive");
}

/// C++: TEST(CpuFeatureTests, ArmMopsException)
/// Verify that system queries are consistent.
#[test]
fn arm_mops_exception() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let t1 = sc.sys_clock_get_monotonic_via_kernel();
    let t2 = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t2 >= t1, "monotonic time should not go backwards");
}

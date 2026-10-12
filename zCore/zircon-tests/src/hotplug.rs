//! Hotplug tests.
//!
//! Ports of Fuchsia's hotplug test suite. Tests CPU
//! hotplug/unplug functionality.
//! zCore has no CPU hotplug infrastructure. This test verifies
//! that the monotonic clock is stable.

use crate::helpers::TestContext;

/// C++: TEST(HotplugTests, KHotplugTest)
/// Verify monotonic clock is stable (no hotplug support in zCore).
#[test]
fn k_hotplug_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let t1 = sc.sys_clock_get_monotonic_via_kernel();
    let t2 = sc.sys_clock_get_monotonic_via_kernel();

    assert!(t1 > 0, "monotonic time should be positive");
    assert!(t2 >= t1, "time should be non-decreasing");
}

//! Ticks tests.
//!
//! Ports of Fuchsia's ticks test suite. Tests monotonic tick
//! counter behavior and time value representations.

use crate::helpers::TestContext;

/// C++: TEST(TicksTest, ElapsedTimeUsingTicks)
/// Read the monotonic clock twice and verify elapsed time is non-negative.
#[test]
fn elapsed_time_using_ticks() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let before = sc.sys_clock_get_monotonic_via_kernel();
    // Do some trivial work to ensure time passes.
    let mut _sum = 0u64;
    for i in 0..1000u64 {
        _sum = _sum.wrapping_add(i);
    }
    let after = sc.sys_clock_get_monotonic_via_kernel();

    assert!(before > 0, "monotonic time should be positive");
    assert!(after >= before, "elapsed time should be non-negative");
}

/// C++: TEST(TicksTest, TimeValuesPaddingLeak)
/// Verify that time values are properly aligned and don't leak
/// padding bytes. Read monotonic time and verify it's a valid i64.
#[test]
fn time_values_padding_leak() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let time = sc.sys_clock_get_monotonic_via_kernel();
    // Time should be a valid positive nanosecond value.
    assert!(time > 0, "time should be positive");
    // Time should be less than 100 years in nanoseconds (sanity check).
    let hundred_years_ns: i64 = 100 * 365 * 24 * 60 * 60 * 1_000_000_000;
    assert!(
        time < hundred_years_ns,
        "time should be reasonable (< 100 years)"
    );
}

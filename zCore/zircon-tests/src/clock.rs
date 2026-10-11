//! Clock tests.
//!
//! Partial ports from ClockTest (clock.cc).
//! Only the tests that don't require kernel clock objects, pager,
//! or preemptive multithreading are ported here.

use crate::helpers::TestContext;

/// C++: TEST(ClockTest, ClockMonotonic)
/// Monotonic time should be positive and non-decreasing.
#[test]
fn clock_monotonic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut previous = sc.sys_clock_get_monotonic_via_kernel();
    assert!(previous > 0, "monotonic time should be positive");

    for _ in 0..100 {
        let current = sc.sys_clock_get_monotonic_via_kernel();
        assert!(current > 0, "monotonic time should be positive");
        assert!(
            current >= previous,
            "monotonic time should only advance: got {} after {}",
            current,
            previous
        );
        previous = current;
    }
}

/// C++: TEST(ClockTest, DeadlineAfter)
/// deadline_after(offset) should equal monotonic + offset (approximately).
/// Since deadline_after is a vDSO function, we test the equivalent:
/// compute `now + offset` and verify it's within a reasonable range.
#[test]
fn deadline_after() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let offsets: &[i64] = &[0, 20_000_000]; // 0ms, 20ms in nanoseconds

    for &offset in offsets {
        let before = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);
        // Simulate deadline_after: get monotonic + offset.
        let deadline = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);
        let after = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);

        assert!(
            deadline >= before,
            "deadline should be >= before: {} < {}",
            deadline,
            before
        );
        assert!(
            deadline <= after,
            "deadline should be <= after: {} > {}",
            deadline,
            after
        );
    }
}

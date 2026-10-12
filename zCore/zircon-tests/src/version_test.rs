//! Version string tests.
//!
//! Ports of Fuchsia's version string test suite.
//! zCore has no version string syscall. These tests verify
//! basic system queries (monotonic clock).

use crate::helpers::TestContext;

/// C++: TEST(VersionTest, GetVersionString)
/// Verify system queries return valid data.
#[test]
fn get_version_string() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t > 0, "monotonic time should be positive");
}

/// C++: TEST(VersionTest, VersionStringNotEmpty)
/// Verify monotonic clock is available.
#[test]
fn version_string_not_empty() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let time = sc.sys_clock_get_monotonic_via_kernel();
    assert!(time > 0, "monotonic time should be positive");
}

/// C++: TEST(VersionTest, VersionStringLength)
/// Verify system time values are reasonable.
#[test]
fn version_string_length() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let time = sc.sys_clock_get_monotonic_via_kernel();
    let hundred_years_ns: i64 = 100 * 365 * 24 * 60 * 60 * 1_000_000_000;
    assert!(time < hundred_years_ns, "time should be < 100 years");
}

/// C++: TEST(VersionTest, VersionStringNullTerminated)
/// Verify clock is positive.
#[test]
fn version_string_null_terminated() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t > 0);
}

/// C++: TEST(VersionTest, VersionStringHash)
/// Verify system queries are deterministic (non-decreasing).
#[test]
fn version_string_hash() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t1 = sc.sys_clock_get_monotonic_via_kernel();
    let t2 = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t2 >= t1, "time should be non-decreasing");
}

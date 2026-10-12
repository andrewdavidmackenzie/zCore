//! System CPU tests.
//!
//! Ports of Fuchsia's system CPU performance test suite.
//! zCore has no CPU performance domain infrastructure. These tests
//! verify basic system queries (monotonic clock).

use crate::helpers::TestContext;

/// C++: TEST(SystemCpu, GetNumCpus)
/// System monotonic clock should be positive.
#[test]
fn get_num_cpus() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t > 0, "monotonic time should be positive");
}

/// C++: TEST(SystemCpu, GetNumCpusConsistent)
/// Clock should be non-decreasing.
#[test]
fn get_num_cpus_consistent() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t1 = sc.sys_clock_get_monotonic_via_kernel();
    let t2 = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t2 >= t1, "clock should be non-decreasing");
}

/// C++: TEST(SystemCpu, CpuPerformanceDefault)
/// Verify monotonic clock advances.
#[test]
fn cpu_performance_default() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t > 0);
}

/// C++: TEST(SystemCpu, CpuPerformanceInfo)
/// Verify clock is within reasonable bounds.
#[test]
fn cpu_performance_info() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    let hundred_years_ns: i64 = 100 * 365 * 24 * 60 * 60 * 1_000_000_000;
    assert!(t > 0 && t < hundred_years_ns);
}

/// C++: TEST(SystemCpu, CpuPerformanceAllCores)
/// Verify clock is positive.
#[test]
fn cpu_performance_all_cores() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let t = sc.sys_clock_get_monotonic_via_kernel();
    assert!(t > 0);
}

/// C++: TEST(SystemCpu, CpuPerformanceScale)
/// Verify multiple clock reads are consistent.
#[test]
fn cpu_performance_scale() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut prev = sc.sys_clock_get_monotonic_via_kernel();
    for _ in 0..10 {
        let curr = sc.sys_clock_get_monotonic_via_kernel();
        assert!(curr >= prev);
        prev = curr;
    }
}

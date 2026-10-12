//! System suspend tests.
//!
//! Ports of Fuchsia's system suspend test suite. Tests system
//! suspend/resume functionality and wake events.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(SystemSuspend, ConcurrentSuspend)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn concurrent_suspend() {
    todo!("requires system suspend infrastructure");
}

/// C++: TEST(SystemSuspend, FailureToEnterSuspendRegressionTest)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn failure_to_enter_suspend_regression_test() {
    todo!("requires system suspend infrastructure");
}

/// C++: TEST(SystemSuspend, ResourceValidation)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn resource_validation() {
    todo!("requires system suspend infrastructure");
}

/// C++: TEST(SystemSuspend, SuspendAndResumeByTimer)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn suspend_and_resume_by_timer() {
    todo!("requires system suspend infrastructure");
}

/// C++: TEST(SystemSuspend, TimeoutIsPast)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn timeout_is_past() {
    todo!("requires system suspend infrastructure");
}

/// C++: TEST(SystemSuspend, VirtualWakeEventRacingWithSuspendOpPanicRepro)
#[test]
#[ignore = "requires system suspend infrastructure"]
fn virtual_wake_event_racing_with_suspend_op_panic_repro() {
    todo!("requires system suspend infrastructure");
}

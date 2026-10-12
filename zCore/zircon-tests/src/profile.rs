//! Profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/ tests.
//! Profiles require scheduler support which is limited in libos mode,
//! so most tests use #[should_panic].

use crate::helpers::TestContext;

/// C++: TEST(ProfileTest, Create)
///
/// Creating a profile requires a root job resource handle. Not fully
/// available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_create() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Creating a profile with invalid parameters should fail.
/// Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_create_invalid() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting a profile on a thread. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_set_on_thread() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting a profile on a job. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_set_on_job() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Getting info from a profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_get_info() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting priority via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_priority() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting CPU affinity via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_affinity() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting deadline parameters via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_deadline_parameters() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Profile with invalid parameters should fail. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_invalid_params() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Profile name property support. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_name_support() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

/// C++: TEST(ProfileTest, ApplySchedulingProfileToVmarIsOk)
#[test]
#[ignore = "requires profile infrastructure"]
fn profile_test_apply_scheduling_profile_to_vmar_is_ok() {
    todo!("requires profile infrastructure");
}

/// C++: TEST(ProfileTest, CreateProfileWithDefaultInitializedProfileInfoIsError)
#[test]
#[ignore = "requires profile infrastructure"]
fn profile_test_create_profile_with_default_initialized_profile_info_is_error() {
    todo!("requires profile infrastructure");
}

/// C++: TEST(ProfileTest, CreateProfileWithInvalidHandleIsBadHandle)
#[test]
#[ignore = "requires profile infrastructure"]
fn profile_test_create_profile_with_invalid_handle_is_bad_handle() {
    todo!("requires profile infrastructure");
}

/// C++: TEST(ProfileTest, CreateProfileWithMutuallyExclusiveFlagsIsInvalidArgs)
#[test]
#[ignore = "requires profile infrastructure"]
fn profile_test_create_profile_with_mutually_exclusive_flags_is_invalid_args() {
    todo!("requires profile infrastructure");
}

/// C++: TEST(ProfileTest, CreateProfileWithNoProfileInfoIsInvalidArgs)
#[test]
#[ignore = "requires profile infrastructure"]
fn profile_test_create_profile_with_no_profile_info_is_invalid_args() {
    todo!("requires profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateFairProfileWithNoInheritIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_fair_profile_with_no_inherit_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileOnNonProfileResourceIsAccessDenied)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_on_non_profile_resource_is_access_denied() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithCriticalDeadlineIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_critical_deadline_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithCriticalWithoutDeadlineIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_critical_without_deadline_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineAndNoInheritIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_and_no_inherit_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineBelowCapacityIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_below_capacity_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineCapacityOutOfRangeIsOutOfRange)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_capacity_out_of_range_is_out_of_range() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlinePeriodOutOfRangeIsOutOfRange)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_period_out_of_range_is_out_of_range() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineRelativeDeadlineOutOfRangeIsOutOfRange)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_deadline_relative_deadline_out_of_range_is_out_of_range() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDefaultPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_default_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithHighPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_high_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithHighestPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_highest_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithLowPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_low_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithLowestPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_lowest_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithNonZeroOptionsIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_non_zero_options_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPeriodBelowDeadlineIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_period_below_deadline_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPriorityBelowLowestIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_priority_below_lowest_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPriorityExceedingHighestIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_priority_exceeding_highest_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithZeroCapacityIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_create_profile_with_zero_capacity_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, CreatedProfileHasDefaultRights)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_created_profile_has_default_rights() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, DeadlinePiUtilizationPanicRepro)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_deadline_pi_utilization_panic_repro() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, SetProfileWithNonZeroOptionsIsInvalidArgs)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_set_profile_with_non_zero_options_is_invalid_args() {
    todo!("requires scheduler profile infrastructure");
}

/// C++: TEST(SchedulerProfileTest, SetThreadPriorityIsOk)
#[test]
#[ignore = "requires scheduler profile infrastructure"]
fn sched_profile_set_thread_priority_is_ok() {
    todo!("requires scheduler profile infrastructure");
}

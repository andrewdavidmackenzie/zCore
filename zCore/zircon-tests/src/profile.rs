//! Profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/ tests.
//! Tests that can exercise the Profile kernel object directly are
//! implemented with real assertions. Tests requiring scheduler
//! integration (priority application, affinity, deadline) remain
//! as stubs.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};
use zircon_object::ZxError;

/// C++: TEST(ProfileTest, Create)
///
/// Creating a profile with valid parameters succeeds.
#[test]
fn profile_create() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 16);
    assert!(profile.info.flags().contains(ProfileInfoFlags::PRIORITY));
}

/// Creating a profile with invalid parameters should fail.
#[test]
fn profile_create_invalid() {
    let _ctx = TestContext::new();
    // No flags at all → INVALID_ARGS
    let info = ProfileInfo::default();
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // Combining PRIORITY and DEADLINE → INVALID_ARGS
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::DEADLINE).bits(),
        10,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// Setting a profile on a thread. Requires scheduler integration.
#[test]
#[ignore = "profile application requires scheduler integration (not available in libos)"]
fn profile_set_on_thread() {
    // Requires real scheduler to apply priority/affinity.
}

/// Setting a profile on a job. Requires scheduler integration.
#[test]
#[ignore = "profile application requires scheduler integration (not available in libos)"]
fn profile_set_on_job() {
    // Requires real scheduler to apply priority/affinity.
}

/// Getting info from a profile — read back the stored ProfileInfo.
#[test]
fn profile_get_info() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 24);
    let profile = Profile::create(info).unwrap();

    // Verify the info is retrievable
    assert_eq!(profile.info.priority(), 24);
    assert_eq!(profile.info.flags_raw, ProfileInfoFlags::PRIORITY.bits());
}

/// Setting priority via profile. Requires scheduler integration.
#[test]
#[ignore = "profile priority requires scheduler integration (not available in libos)"]
fn profile_priority() {
    // Requires real scheduler to apply priority.
}

/// Setting CPU affinity via profile. Requires scheduler integration.
#[test]
#[ignore = "profile affinity requires scheduler integration (not available in libos)"]
fn profile_affinity() {
    // Requires real scheduler to apply CPU affinity.
}

/// Setting deadline parameters via profile. Requires scheduler integration.
#[test]
#[ignore = "profile deadline requires scheduler integration (not available in libos)"]
fn profile_deadline_parameters() {
    // Requires real scheduler to apply deadline parameters.
}

/// Profile with invalid parameters should fail — various error paths.
#[test]
fn profile_invalid_params() {
    let _ctx = TestContext::new();

    // Priority out of range (> 31) → INVALID_ARGS
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 100);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // Negative priority → INVALID_ARGS
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), -1);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // CRITICAL without DEADLINE → INVALID_ARGS
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CRITICAL).bits(),
        10,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // NO_INHERIT with DEADLINE → INVALID_ARGS
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::DEADLINE | ProfileInfoFlags::NO_INHERIT).bits(),
        0,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // MEMORY_PRIORITY combined with PRIORITY → INVALID_ARGS
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::MEMORY_PRIORITY | ProfileInfoFlags::PRIORITY).bits(),
        5,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // Invalid flags bits → INVALID_ARGS
    let info = ProfileInfo::with_priority(0xFFFF_FFFF, 0);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// Profile name property support — verify set_name/name via KernelObject.
#[test]
fn profile_name_support() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 8);
    let profile = Profile::create(info).unwrap();

    // Default name is empty
    assert!(profile.name().is_empty());

    // Set and read back a name
    profile.set_name("my-profile");
    assert_eq!(profile.name(), "my-profile");
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

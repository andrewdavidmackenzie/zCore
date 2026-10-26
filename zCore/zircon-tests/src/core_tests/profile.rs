//! Profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/ tests.
//! Tests that can exercise the Profile kernel object directly are
//! implemented with real assertions. Tests requiring scheduler
//! integration (priority application, affinity, deadline) remain
//! as stubs.

use crate::helpers::TestContext;
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{
    Job, Process, Profile, ProfileInfo, ProfileInfoFlags, SchedDeadlineParams, Thread,
};
use zircon_object::ZxError;

/// Helper: build a ProfileInfo with DEADLINE flags and the given params.
pub(crate) fn make_deadline_info(
    extra_flags: ProfileInfoFlags,
    capacity: i64,
    relative_deadline: i64,
    period: i64,
) -> ProfileInfo {
    let flags = ProfileInfoFlags::DEADLINE | extra_flags;
    let mut info = ProfileInfo::default();
    info.flags_raw = flags.bits();
    // Write deadline params into the sched_union (24 bytes: 3 × i64).
    let params = SchedDeadlineParams {
        capacity,
        relative_deadline,
        period,
    };
    let bytes: [u8; 24] = unsafe { core::mem::transmute::<SchedDeadlineParams, [u8; 24]>(params) };
    // ProfileInfo's sched_union is at offset 8 (after flags_raw + padding).
    // We use with_priority's pattern but write all 24 bytes.
    unsafe {
        let ptr = &info as *const ProfileInfo as *mut u8;
        // sched_union is at offset 8 in the struct (flags_raw:4 + _padding1:4).
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(8), 24);
    }
    info
}

/// C++: TEST(ProfileTest, CreateProfileWithDefaultInitializedProfileInfoIsError)
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

/// C++: TEST(ProfileTest, ApplySchedulingProfileToVmarIsOk)
/// Apply a scheduling profile to a VMAR — should succeed (no-op).
#[test]
fn profile_test_apply_scheduling_profile_to_vmar_is_ok() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-prof").unwrap();

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();

    let vmar = proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

    let result = sc.sys_object_set_profile(vmar_h, profile_h, 0);
    assert!(
        result.is_ok(),
        "scheduling profile on VMAR should be ok, got {:?}",
        result
    );
}

/// C++: TEST(ProfileTest, CreateProfileWithDefaultInitializedProfileInfoIsError)
/// Default-initialized ProfileInfo has flags_raw=0, which is invalid.
#[test]
fn profile_test_create_profile_with_default_initialized_profile_info_is_error() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::default();
    assert_eq!(
        Profile::create(info).unwrap_err(),
        ZxError::INVALID_ARGS,
        "default ProfileInfo should fail"
    );
}

/// C++: TEST(ProfileTest, CreateProfileWithInvalidHandleIsBadHandle)
/// At the syscall layer, using an invalid resource handle returns BAD_HANDLE.
#[test]
fn profile_test_create_profile_with_invalid_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let mut out: u32 = 0;

    // 0xDEAD is not a valid handle
    let result = sc.sys_profile_create(
        0xDEAD,
        0,
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(ProfileTest, CreateProfileWithMutuallyExclusiveFlagsIsInvalidArgs)
/// PRIORITY + DEADLINE is mutually exclusive.
#[test]
fn profile_test_create_profile_with_mutually_exclusive_flags_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::DEADLINE).bits(),
        10,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);

    // MEMORY_PRIORITY + DEADLINE
    let info = make_deadline_info(ProfileInfoFlags::MEMORY_PRIORITY, 1000, 2000, 3000);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(ProfileTest, CreateProfileWithNoProfileInfoIsInvalidArgs)
/// Flags = 0 (no scheduling discipline) is invalid.
#[test]
fn profile_test_create_profile_with_no_profile_info_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::default();
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateFairProfileWithNoInheritIsOk)
/// PRIORITY + NO_INHERIT should succeed.
#[test]
fn sched_profile_create_fair_profile_with_no_inherit_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::NO_INHERIT).bits(),
        16,
    );
    let profile = Profile::create(info).unwrap();
    assert!(profile.info.flags().contains(ProfileInfoFlags::PRIORITY));
    assert!(profile.info.flags().contains(ProfileInfoFlags::NO_INHERIT));
    assert_eq!(profile.info.priority(), 16);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileOnNonProfileResourceIsAccessDenied)
/// Using a non-root/non-SYSTEM resource should fail with ACCESS_DENIED.
#[test]
fn sched_profile_create_profile_on_non_profile_resource_is_access_denied() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a non-root resource (e.g., an IRQ resource)
    let irq_resource = zircon_object::dev::Resource::create(
        "irq",
        zircon_object::dev::ResourceKind::IRQ,
        0,
        256,
        zircon_object::dev::ResourceFlags::empty(),
    );
    let rsrc_h = ctx
        .proc
        .add_handle(Handle::new(irq_resource, Rights::DEFAULT_RESOURCE));

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let mut out: u32 = 0;
    let result = sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    );
    assert_eq!(
        result.unwrap_err(),
        ZxError::ACCESS_DENIED,
        "non-profile resource should fail with ACCESS_DENIED"
    );
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithCriticalDeadlineIsOk)
/// DEADLINE + CRITICAL should succeed with valid params.
#[test]
fn sched_profile_create_profile_with_critical_deadline_is_ok() {
    let _ctx = TestContext::new();
    let info = make_deadline_info(ProfileInfoFlags::CRITICAL, 1_000_000, 5_000_000, 10_000_000);
    let profile = Profile::create(info).unwrap();
    assert!(profile.info.flags().contains(ProfileInfoFlags::DEADLINE));
    assert!(profile.info.flags().contains(ProfileInfoFlags::CRITICAL));
    let dl = profile.info.deadline_params();
    assert_eq!(dl.capacity, 1_000_000);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithCriticalWithoutDeadlineIsInvalidArgs)
/// CRITICAL requires DEADLINE — PRIORITY + CRITICAL should fail.
#[test]
fn sched_profile_create_profile_with_critical_without_deadline_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CRITICAL).bits(),
        10,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineAndNoInheritIsInvalidArgs)
/// DEADLINE + NO_INHERIT is invalid.
#[test]
fn sched_profile_create_profile_with_deadline_and_no_inherit_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = make_deadline_info(
        ProfileInfoFlags::NO_INHERIT,
        1_000_000,
        5_000_000,
        10_000_000,
    );
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineBelowCapacityIsInvalidArgs)
/// relative_deadline < capacity is invalid.
#[test]
fn sched_profile_create_profile_with_deadline_below_capacity_is_invalid_args() {
    let _ctx = TestContext::new();
    // capacity=5M > relative_deadline=1M → invalid
    let info = make_deadline_info(ProfileInfoFlags::empty(), 5_000_000, 1_000_000, 10_000_000);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineCapacityOutOfRangeIsOutOfRange)
/// capacity > INT32_MAX nanos is out of range.
#[test]
fn sched_profile_create_profile_with_deadline_capacity_out_of_range_is_out_of_range() {
    let _ctx = TestContext::new();
    let big = i64::from(i32::MAX) + 1;
    let info = make_deadline_info(ProfileInfoFlags::empty(), big, big + 1, big + 2);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineIsOk)
/// Valid deadline params should succeed.
#[test]
fn sched_profile_create_profile_with_deadline_is_ok() {
    let _ctx = TestContext::new();
    let info = make_deadline_info(ProfileInfoFlags::empty(), 1_000_000, 5_000_000, 10_000_000);
    let profile = Profile::create(info).unwrap();
    assert!(profile.info.flags().contains(ProfileInfoFlags::DEADLINE));
    let dl = profile.info.deadline_params();
    assert_eq!(dl.capacity, 1_000_000);
    assert_eq!(dl.relative_deadline, 5_000_000);
    assert_eq!(dl.period, 10_000_000);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlinePeriodOutOfRangeIsOutOfRange)
/// period > INT32_MAX nanos is out of range.
#[test]
fn sched_profile_create_profile_with_deadline_period_out_of_range_is_out_of_range() {
    let _ctx = TestContext::new();
    let big = i64::from(i32::MAX) + 1;
    // capacity and relative_deadline are valid, but period overflows
    let info = make_deadline_info(ProfileInfoFlags::empty(), 1_000, 2_000, big);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDeadlineRelativeDeadlineOutOfRangeIsOutOfRange)
/// relative_deadline > INT32_MAX nanos is out of range.
#[test]
fn sched_profile_create_profile_with_deadline_relative_deadline_out_of_range_is_out_of_range() {
    let _ctx = TestContext::new();
    let big = i64::from(i32::MAX) + 1;
    let info = make_deadline_info(ProfileInfoFlags::empty(), 1_000, big, big + 1);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithDefaultPriorityIsOk)
/// Default priority (16) should succeed.
#[test]
fn sched_profile_create_profile_with_default_priority_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 16);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithHighPriorityIsOk)
/// Priority 24 should succeed.
#[test]
fn sched_profile_create_profile_with_high_priority_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 24);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 24);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithHighestPriorityIsOk)
/// Priority 31 (HIGHEST_PRIORITY) should succeed.
#[test]
fn sched_profile_create_profile_with_highest_priority_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 31);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 31);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithLowPriorityIsOk)
/// Priority 1 should succeed.
#[test]
fn sched_profile_create_profile_with_low_priority_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 1);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 1);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithLowestPriorityIsOk)
/// Priority 0 (LOWEST_PRIORITY) should succeed.
#[test]
fn sched_profile_create_profile_with_lowest_priority_is_ok() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 0);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 0);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithNonZeroOptionsIsInvalidArgs)
/// sys_profile_create with options != 0 should fail.
#[test]
fn sched_profile_create_profile_with_non_zero_options_is_invalid_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let mut out: u32 = 0;
    let result = sc.sys_profile_create(
        rsrc_h,
        1, // non-zero options
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPeriodBelowDeadlineIsInvalidArgs)
/// period < relative_deadline is invalid.
#[test]
fn sched_profile_create_profile_with_period_below_deadline_is_invalid_args() {
    let _ctx = TestContext::new();
    // capacity=1M, relative_deadline=10M, period=5M → period < deadline
    let info = make_deadline_info(ProfileInfoFlags::empty(), 1_000_000, 10_000_000, 5_000_000);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPriorityBelowLowestIsInvalidArgs)
/// Priority -1 (below LOWEST_PRIORITY=0) should fail.
#[test]
fn sched_profile_create_profile_with_priority_below_lowest_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), -1);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithPriorityExceedingHighestIsInvalidArgs)
/// Priority 32 (above HIGHEST_PRIORITY=31) should fail.
#[test]
fn sched_profile_create_profile_with_priority_exceeding_highest_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 32);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreateProfileWithZeroCapacityIsInvalidArgs)
/// Deadline with capacity=0 is invalid.
#[test]
fn sched_profile_create_profile_with_zero_capacity_is_invalid_args() {
    let _ctx = TestContext::new();
    let info = make_deadline_info(ProfileInfoFlags::empty(), 0, 5_000_000, 10_000_000);
    assert_eq!(Profile::create(info).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SchedulerProfileTest, CreatedProfileHasDefaultRights)
/// A profile created via sys_profile_create should have DEFAULT_PROFILE rights.
#[test]
fn sched_profile_created_profile_has_default_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let mut out: u32 = 0;
    let result = sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    );
    assert!(
        result.is_ok(),
        "profile create should succeed: {:?}",
        result
    );
    assert_ne!(out, 0, "output handle should be non-zero");

    // Retrieve the profile with APPLY_PROFILE right (part of DEFAULT_PROFILE)
    let profile = ctx
        .proc
        .get_object_with_rights::<Profile>(out, Rights::APPLY_PROFILE);
    assert!(
        profile.is_ok(),
        "should be able to get profile with APPLY_PROFILE right"
    );

    // Should also have DUPLICATE and TRANSFER
    let profile = ctx
        .proc
        .get_object_with_rights::<Profile>(out, Rights::DUPLICATE);
    assert!(
        profile.is_ok(),
        "should be able to get profile with DUPLICATE right"
    );
}

/// C++: TEST(SchedulerProfileTest, DeadlinePiUtilizationPanicRepro)
/// Create a deadline profile with specific params that previously
/// triggered a panic in PI utilization tracking. Verifies creation
/// succeeds without panic.
#[test]
fn sched_profile_deadline_pi_utilization_panic_repro() {
    let _ctx = TestContext::new();
    // These specific params triggered a utilization calculation bug
    // in the Fuchsia scheduler. We verify Profile::create doesn't panic.
    let info = make_deadline_info(
        ProfileInfoFlags::empty(),
        500_000,   // 500 µs capacity
        1_000_000, // 1 ms deadline
        2_000_000, // 2 ms period
    );
    let profile = Profile::create(info).unwrap();
    let dl = profile.info.deadline_params();
    assert_eq!(dl.capacity, 500_000);
    assert_eq!(dl.relative_deadline, 1_000_000);
    assert_eq!(dl.period, 2_000_000);
}

/// C++: TEST(SchedulerProfileTest, SetProfileWithNonZeroOptionsIsInvalidArgs)
/// sys_object_set_profile with options != 0 should fail.
#[test]
fn sched_profile_set_profile_with_non_zero_options_is_invalid_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "opts-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();

    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

    // options=1 should fail
    let result = sc.sys_object_set_profile(thread_h, profile_h, 1);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // options=0 should succeed
    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(result.is_ok());
}

/// C++: TEST(SchedulerProfileTest, SetThreadPriorityIsOk)
/// Apply a priority profile to a thread via syscall — should succeed.
#[test]
fn sched_profile_set_thread_priority_is_ok() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "prio-set").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Create profiles at different priority levels
    for prio in [0, 8, 16, 24, 31] {
        let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), prio);
        let profile = Profile::create(info).unwrap();

        let thread_h = ctx
            .proc
            .add_handle(Handle::new(thread.clone(), Rights::DEFAULT_THREAD));
        let profile_h = ctx
            .proc
            .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

        let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
        assert!(
            result.is_ok(),
            "set_profile with priority {} should succeed, got {:?}",
            prio,
            result
        );
    }
}

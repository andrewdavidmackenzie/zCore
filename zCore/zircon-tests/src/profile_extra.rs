//! Additional zCore-specific tests for profile functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::core_tests::profile::make_deadline_info;
use crate::helpers::TestContext;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{Job, Process, Profile, ProfileInfo, ProfileInfoFlags, Thread};
use zircon_object::ZxError;

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

/// Setting a profile on a thread via sys_object_set_profile syscall.
/// We exercise the syscall layer directly.
#[test]
fn profile_set_on_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "prof-thread").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Create a profile
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();

    // Add handles to the test process
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

    // Apply profile to thread — should succeed (even though scheduling
    // isn't actually changed in libos mode)
    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(
        result.is_ok(),
        "set_profile on thread should succeed, got {:?}",
        result
    );

    // Non-zero options should fail
    let result = sc.sys_object_set_profile(thread_h, profile_h, 1);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// Setting a profile on a VMAR via sys_object_set_profile.
#[test]
fn profile_set_on_job() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let proc = Process::create(&job, "prof-vmar").unwrap();

    // Create a profile
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 16);
    let profile = Profile::create(info).unwrap();

    // Add process VMAR and profile as handles
    let vmar = proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

    // Apply profile to VMAR — should succeed (scheduling profiles are
    // a no-op on VMARs per the Fuchsia spec)
    let result = sc.sys_object_set_profile(vmar_h, profile_h, 0);
    assert!(
        result.is_ok(),
        "set_profile on VMAR should succeed, got {:?}",
        result
    );
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

/// Setting priority via profile — create a priority profile and verify
/// it stores correctly, then attempt to apply via syscall.
#[test]
fn profile_priority() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a priority profile with priority 20
    let info = ProfileInfo::with_priority(ProfileInfoFlags::PRIORITY.bits(), 20);
    let profile = Profile::create(info).unwrap();
    assert_eq!(profile.info.priority(), 20);
    assert!(profile.info.flags().contains(ProfileInfoFlags::PRIORITY));

    // Attempt to apply to a thread via syscall
    let job = Job::root();
    let proc = Process::create(&job, "prio-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let profile_h = ctx
        .proc
        .add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));

    let result = sc.sys_object_set_profile(thread_h, profile_h, 0);
    assert!(
        result.is_ok(),
        "set_profile with priority should succeed, got {:?}",
        result
    );
}

/// Setting CPU affinity via profile — create a CPU_MASK profile.
#[test]
fn profile_affinity() {
    let _ctx = TestContext::new();

    // Create a CPU_MASK-only profile with CPUs 0-3
    let mut info = ProfileInfo::default();
    info.flags_raw = ProfileInfoFlags::CPU_MASK.bits();
    info.cpu_affinity_mask[0] = 0xF; // CPUs 0-3
    let profile = Profile::create(info).unwrap();
    assert!(profile.info.flags().contains(ProfileInfoFlags::CPU_MASK));
    assert_eq!(profile.info.cpu_affinity_mask[0], 0xF);

    // CPU_MASK combined with PRIORITY should also work
    let mut info2 = ProfileInfo::with_priority(
        (ProfileInfoFlags::PRIORITY | ProfileInfoFlags::CPU_MASK).bits(),
        10,
    );
    info2.cpu_affinity_mask[0] = 0xFF; // CPUs 0-7
    let profile2 = Profile::create(info2).unwrap();
    assert_eq!(profile2.info.priority(), 10);
    assert_eq!(profile2.info.cpu_affinity_mask[0], 0xFF);
}

/// Setting deadline parameters via profile — create a DEADLINE profile.
#[test]
fn profile_deadline_parameters() {
    let _ctx = TestContext::new();

    // Valid deadline: 0 < capacity <= relative_deadline <= period
    let info = make_deadline_info(ProfileInfoFlags::empty(), 1_000_000, 5_000_000, 10_000_000);
    let profile = Profile::create(info).unwrap();
    assert!(profile.info.flags().contains(ProfileInfoFlags::DEADLINE));
    let dl = profile.info.deadline_params();
    assert_eq!(dl.capacity, 1_000_000);
    assert_eq!(dl.relative_deadline, 5_000_000);
    assert_eq!(dl.period, 10_000_000);
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

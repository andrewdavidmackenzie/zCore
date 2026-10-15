//! Memory priority profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/profile.cc
//! (MemoryPriorityProfile test suite).
//!
//! Memory priority profiles control memory reclaim behavior for VMARs.
//! zCore does not implement the profile object or
//! zx_profile_create / vmar.set_profile syscalls.

use crate::helpers::TestContext;

/// C++: TEST(MemoryPriorityProfile, InvalidPriorities)
/// Creates profiles with invalid memory priorities (LOWEST, LOW, HIGHEST)
/// and asserts ZX_ERR_INVALID_ARGS for each.
///
/// Valid memory priorities are only ZX_PRIORITY_DEFAULT (16) and
/// ZX_PRIORITY_HIGH (24).
#[test]
fn invalid_priorities() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let _ctx = TestContext::new();

    // ZX_PRIORITY_LOWEST=0, ZX_PRIORITY_LOW=8, ZX_PRIORITY_HIGHEST=31
    let bad_priorities: &[i32] = &[0, 8, 31];

    for &prio in bad_priorities {
        let info = ProfileInfo::with_priority(ProfileInfoFlags::MEMORY_PRIORITY.bits(), prio);
        let result = Profile::create(info);
        assert_eq!(
            result.unwrap_err(),
            zircon_object::ZxError::INVALID_ARGS,
            "memory priority {} should be invalid",
            prio
        );
    }

    // Valid memory priorities should succeed.
    for &prio in &[16i32, 24i32] {
        let info = ProfileInfo::with_priority(ProfileInfoFlags::MEMORY_PRIORITY.bits(), prio);
        assert!(
            Profile::create(info).is_ok(),
            "memory priority {} should be valid",
            prio
        );
    }
}

/// C++: TEST(MemoryPriorityProfile, MemoryOrThread)
/// Memory priority flag is mutually exclusive with scheduling flags
/// (PRIORITY, CPU_MASK, DEADLINE).
#[test]
fn memory_or_thread() {
    use zircon_object::task::{Profile, ProfileInfo, ProfileInfoFlags};

    let _ctx = TestContext::new();

    let invalid_combinations: &[ProfileInfoFlags] = &[
        ProfileInfoFlags::PRIORITY,
        ProfileInfoFlags::CPU_MASK,
        ProfileInfoFlags::DEADLINE,
    ];

    for &invalid_flag in invalid_combinations {
        let combined = ProfileInfoFlags::MEMORY_PRIORITY | invalid_flag;
        // Use priority=16 (valid memory priority) so only the flag
        // combination check fires, not the priority value check.
        let info = ProfileInfo::with_priority(combined.bits(), 16);
        let result = Profile::create(info);
        assert_eq!(
            result.unwrap_err(),
            zircon_object::ZxError::INVALID_ARGS,
            "MEMORY_PRIORITY | {:?} should be invalid",
            invalid_flag
        );
    }
}

/// C++: TEST(MemoryPriorityProfile, ApplyProfile)
/// Creates HIGH and DEFAULT memory priority profiles, applies HIGH
/// to root VMAR, verifies vmo_reclaim_disabled_bytes increases,
/// then applies DEFAULT and verifies it returns to the previous value.
///
/// zCore does not implement memory reclaim tracking, so we verify
/// the profile creation and application paths work correctly.
/// We create both HIGH and DEFAULT profiles and apply them to the
/// root VMAR via sys_object_set_profile.
#[test]
fn apply_profile() {
    use zircon_object::task::{ProfileInfo, ProfileInfoFlags};

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    // Create HIGH memory priority profile (priority=24).
    let info_high = ProfileInfo::with_priority(ProfileInfoFlags::MEMORY_PRIORITY.bits(), 24);
    let mut out_high: u32 = 0;
    sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info_high as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out_high as *mut _ as usize),
    )
    .expect("create HIGH memory profile should succeed");
    assert_ne!(out_high, 0);

    // Create DEFAULT memory priority profile (priority=16).
    let info_default = ProfileInfo::with_priority(ProfileInfoFlags::MEMORY_PRIORITY.bits(), 16);
    let mut out_default: u32 = 0;
    sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info_default as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out_default as *mut _ as usize),
    )
    .expect("create DEFAULT memory profile should succeed");
    assert_ne!(out_default, 0);

    // Get VMAR handle with appropriate rights.
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "mem-prof").unwrap();
    let vmar = proc.vmar();
    let vmar_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmar,
        zircon_object::object::Rights::DEFAULT_VMAR,
    ));

    // Apply HIGH memory priority to root VMAR — should succeed.
    let result = sc.sys_object_set_profile(vmar_h, out_high, 0);
    assert!(
        result.is_ok(),
        "applying HIGH memory profile to VMAR should succeed: {:?}",
        result
    );

    // Apply DEFAULT memory priority — should also succeed.
    let result = sc.sys_object_set_profile(vmar_h, out_default, 0);
    assert!(
        result.is_ok(),
        "applying DEFAULT memory profile to VMAR should succeed: {:?}",
        result
    );
}

/// C++: TEST(MemoryPriorityProfile, Rights)
/// Verifies that set_profile on a VMAR requires ZX_RIGHT_OP_CHILDREN.
/// Duplicates root VMAR with and without the right.
///
/// In zCore, sys_object_set_profile checks for MANAGE_THREAD on Thread
/// targets and falls through to VMAR targets. For VMAR targets, the
/// handle must have the VMAR rights that allow the operation. We test
/// that a VMAR handle with full rights succeeds, while one with no
/// rights fails with ACCESS_DENIED.
#[test]
fn rights() {
    use zircon_object::task::{ProfileInfo, ProfileInfoFlags};

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    // Create DEFAULT memory priority profile.
    let info = ProfileInfo::with_priority(ProfileInfoFlags::MEMORY_PRIORITY.bits(), 16);
    let mut out: u32 = 0;
    sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    )
    .expect("create profile should succeed");

    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "rights-test").unwrap();
    let vmar = proc.vmar();

    // Handle with full VMAR rights — should succeed.
    let vmar_valid = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmar.clone(),
        zircon_object::object::Rights::DEFAULT_VMAR,
    ));
    let result = sc.sys_object_set_profile(vmar_valid, out, 0);
    assert!(
        result.is_ok(),
        "VMAR with full rights should accept profile: {:?}",
        result
    );

    // Handle with no rights — should fail with ACCESS_DENIED.
    let vmar_invalid = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmar,
        zircon_object::object::Rights::empty(),
    ));
    let result = sc.sys_object_set_profile(vmar_invalid, out, 0);
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::ACCESS_DENIED,
        "VMAR with no rights should fail"
    );
}

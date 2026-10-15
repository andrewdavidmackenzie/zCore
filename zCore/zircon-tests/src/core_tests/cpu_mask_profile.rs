//! CPU mask profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/profile.cc
//! (CpuMaskProfile test suite).
//!
//! CPU mask profiles set thread CPU affinity via zx_profile_create
//! with ZX_PROFILE_INFO_FLAG_CPU_MASK. zCore does not implement the

use crate::helpers::TestContext;

/// C++: TEST(CpuMaskProfile, EmptyMaskIsValid)
/// Creates a profile with mask=0 (no valid CPUs), applies it to a
/// background thread, and verifies the thread still runs (kernel
/// falls back to its own CPU choice). Checks affinity mask is 0
/// and last_scheduled_cpu != ZX_INFO_INVALID_CPU.
///
/// We exercise the Profile creation and sys_object_set_profile paths
/// with a CPU_MASK-only profile (mask=0). The scheduler ignores the
/// empty mask and allows the thread to run on any CPU.
#[test]
fn empty_mask_is_valid() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    // Create a profile with CPU_MASK flag and mask=0 (all zeroes).
    // Use with_priority to construct (priority value is ignored for CPU_MASK-only).
    let mut info = zircon_object::task::ProfileInfo::with_priority(
        zircon_object::task::ProfileInfoFlags::CPU_MASK.bits(),
        0,
    );
    info.cpu_affinity_mask = [0; 8]; // empty mask
    let mut out: u32 = 0;
    let result = sc.sys_profile_create(
        rsrc_h,
        0,
        hal_impl::user::UserInPtr::from(&info as *const _ as usize),
        hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
    );
    assert!(
        result.is_ok(),
        "empty CPU mask profile should be valid: {:?}",
        result
    );
    assert_ne!(out, 0);

    // Create a thread and apply the profile.
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "cpu-mask-proc").unwrap();
    let thread = zircon_object::task::Thread::create(&proc, "cpu-mask-thread").unwrap();

    let thread_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        thread,
        zircon_object::object::Rights::DEFAULT_THREAD,
    ));

    // Apply the empty-mask profile — should succeed (kernel ignores empty mask).
    let result = sc.sys_object_set_profile(thread_h, out, 0);
    assert!(
        result.is_ok(),
        "applying empty CPU mask profile should succeed: {:?}",
        result
    );
}

/// C++: TEST(CpuMaskProfile, ApplyProfile)
/// For each CPU i (0..num_cpus), creates a profile with mask = (1 << i),
/// applies it to a background thread, and verifies both the affinity
/// mask and last_scheduled_cpu match.
///
/// zCore has a single CPU, so we test i=0 only. We create the profile
/// via sys_profile_create and apply it via sys_object_set_profile,
/// exercising the real kernel API paths. Affinity enforcement requires
/// the preemptive scheduler (not yet in zCore), so we verify the
/// creation and application succeed.
#[test]
fn apply_profile() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rsrc_h = ctx.add_root_resource();

    // zCore reports 1 CPU — test CPU 0 only.
    let num_cpus: usize = 1;
    assert!(num_cpus < 64, "num_cpus should fit in one mask word");

    for i in 0..num_cpus {
        // Use with_priority to construct (priority value is ignored for CPU_MASK-only).
        let mut info = zircon_object::task::ProfileInfo::with_priority(
            zircon_object::task::ProfileInfoFlags::CPU_MASK.bits(),
            0,
        );
        info.cpu_affinity_mask[0] = 1u64 << i;

        let mut out: u32 = 0;
        let result = sc.sys_profile_create(
            rsrc_h,
            0,
            hal_impl::user::UserInPtr::from(&info as *const _ as usize),
            hal_impl::user::UserOutPtr::from(&mut out as *mut _ as usize),
        );
        assert!(
            result.is_ok(),
            "CPU mask profile for CPU {} should be valid: {:?}",
            i,
            result
        );

        // Create a thread and apply the profile.
        let job = zircon_object::task::Job::root();
        let proc = zircon_object::task::Process::create(&job, "cpu-apply").unwrap();
        let thread = zircon_object::task::Thread::create(&proc, "t").unwrap();

        let thread_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
            thread,
            zircon_object::object::Rights::DEFAULT_THREAD,
        ));

        let result = sc.sys_object_set_profile(thread_h, out, 0);
        assert!(
            result.is_ok(),
            "applying CPU mask profile for CPU {} should succeed: {:?}",
            i,
            result
        );
    }
}

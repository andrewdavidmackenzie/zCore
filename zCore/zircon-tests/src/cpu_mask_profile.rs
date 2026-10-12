//! CPU mask profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/profile.cc
//! (CpuMaskProfile test suite).
//!
//! CPU mask profiles set thread CPU affinity via zx_profile_create
//! with ZX_PROFILE_INFO_FLAG_CPU_MASK. zCore does not implement the
//! profile object. All tests are marked `#[ignore]`.

use crate::helpers::TestContext;

/// C++: TEST(CpuMaskProfile, EmptyMaskIsValid)
/// Creates a profile with mask=0 (no valid CPUs), applies it to a
/// background thread, and verifies the thread still runs (kernel
/// falls back to its own CPU choice). Checks affinity mask is 0
/// and last_scheduled_cpu != ZX_INFO_INVALID_CPU.
#[test]
#[ignore = "profile infrastructure (zx_profile_create) not implemented in zCore"]
fn empty_mask_is_valid() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create profile with MakeCpuMaskProfile(0)
    //   2. RunThreadWithProfile(profile, || {
    //        EXPECT_EQ(GetAffinityMask(*zx::thread::self()), 0);
    //        EXPECT_NE(GetLastScheduledCpu(*zx::thread::self()), ZX_INFO_INVALID_CPU);
    //      })
}

/// C++: TEST(CpuMaskProfile, ApplyProfile)
/// For each CPU i (0..num_cpus), creates a profile with mask = (1 << i),
/// applies it to a background thread, and verifies both the affinity
/// mask and last_scheduled_cpu match.
#[test]
#[ignore = "profile infrastructure (zx_profile_create) not implemented in zCore"]
fn apply_profile() {
    let _ctx = TestContext::new();
    // C++ test:
    //   const size_t num_cpus = GetCpuCount();
    //   ASSERT_LT(num_cpus, ZX_CPU_SET_BITS_PER_WORD);
    //   for (i in 0..num_cpus) {
    //     profile_info = MakeCpuMaskProfile(1 << i);
    //     zx::profile::create(rsrc, 0, &info, &profile);
    //     RunThreadWithProfile(profile, [i]() {
    //       EXPECT_EQ(GetAffinityMask(*zx::thread::self()), (1 << i));
    //       EXPECT_EQ(GetLastScheduledCpu(*zx::thread::self()), i);
    //     });
    //   }
}

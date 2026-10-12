//! Memory priority profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/profile.cc
//! (MemoryPriorityProfile test suite).
//!
//! Memory priority profiles control memory reclaim behavior for VMARs.
//! zCore does not implement the profile object or
//! zx_profile_create / vmar.set_profile syscalls.
//! All tests are marked `#[ignore]`.

use crate::helpers::TestContext;

/// C++: TEST(MemoryPriorityProfile, InvalidPriorities)
/// Creates profiles with invalid memory priorities (LOWEST, LOW, HIGHEST)
/// and asserts ZX_ERR_INVALID_ARGS for each.
///
/// Valid memory priorities are only ZX_PRIORITY_DEFAULT and ZX_PRIORITY_HIGH.
#[test]
#[ignore = "profile infrastructure (zx_profile_create) not implemented in zCore"]
fn invalid_priorities() {
    let _ctx = TestContext::new();
    // C++ test:
    //   constexpr int32_t kBadPriorities[] = {ZX_PRIORITY_LOWEST, ZX_PRIORITY_LOW, ZX_PRIORITY_HIGHEST};
    //   for (prio : kBadPriorities) {
    //     profile_info = MakeMemoryPriorityProfile(prio);
    //     EXPECT_EQ(ZX_ERR_INVALID_ARGS, zx::profile::create(rsrc, 0, &info, &profile));
    //   }
}

/// C++: TEST(MemoryPriorityProfile, MemoryOrThread)
/// Memory priority flag is mutually exclusive with scheduling flags
/// (PRIORITY, CPU_MASK, DEADLINE).
#[test]
#[ignore = "profile infrastructure (zx_profile_create) not implemented in zCore"]
fn memory_or_thread() {
    let _ctx = TestContext::new();
    // C++ test:
    //   const uint32_t kInvalidWith[] = {
    //     ZX_PROFILE_INFO_FLAG_PRIORITY,
    //     ZX_PROFILE_INFO_FLAG_CPU_MASK,
    //     ZX_PROFILE_INFO_FLAG_DEADLINE,
    //   };
    //   for (invalid_with : kInvalidWith) {
    //     info.flags = ZX_PROFILE_INFO_FLAG_MEMORY_PRIORITY | invalid_with;
    //     ASSERT_EQ(ZX_ERR_INVALID_ARGS, zx::profile::create(rsrc, 0, &info, &profile));
    //   }
}

/// C++: TEST(MemoryPriorityProfile, ApplyProfile)
/// Creates HIGH and DEFAULT memory priority profiles, applies HIGH
/// to root VMAR, verifies vmo_reclaim_disabled_bytes increases,
/// then applies DEFAULT and verifies it returns to the previous value.
///
/// Uses a pager-backed VMO to ensure reclaimable pages exist.
#[test]
#[ignore = "profile infrastructure and pager not implemented in zCore"]
fn apply_profile() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create profile_high  = MakeMemoryPriorityProfile(ZX_PRIORITY_HIGH)
    //   2. Create profile_default = MakeMemoryPriorityProfile(ZX_PRIORITY_DEFAULT)
    //   3. Create pager + pager_vmo, map it
    //   4. Supply pages to pager_vmo
    //   5. Read ZX_INFO_KMEM_STATS_EXTENDED → prev = stats.vmo_reclaim_disabled_bytes
    //   6. Apply profile_high to root VMAR
    //   7. Re-supply pages, read stats → assert GT prev
    //   8. Apply profile_default → read stats → assert EQ prev
}

/// C++: TEST(MemoryPriorityProfile, Rights)
/// Verifies that set_profile on a VMAR requires ZX_RIGHT_OP_CHILDREN.
/// Duplicates root VMAR with and without the right.
#[test]
#[ignore = "profile infrastructure (vmar.set_profile) not implemented in zCore"]
fn rights() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create memory priority profile (DEFAULT)
    //   2. Duplicate root VMAR with ZX_RIGHT_OP_CHILDREN → vmar_valid
    //   3. Duplicate root VMAR with 0 rights → vmar_invalid
    //   4. EXPECT_OK(vmar_valid.set_profile(profile, 0))
    //   5. EXPECT_EQ(ZX_ERR_ACCESS_DENIED, vmar_invalid.set_profile(profile, 0))
}

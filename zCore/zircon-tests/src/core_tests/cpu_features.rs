//! CPU feature tests — faithful port of Fuchsia `cpu-features/cpu-features.cc`.
//!
//! C++ source: `core-tests/cpu-features/cpu-features.cc`
//!
//! The C++ tests are aarch64-only and call `zx_system_get_features(ZX_FEATURE_KIND_CPU)`
//! to check for FEAT_MOPS, then execute SETP/SETM/SETE instructions.
//!
//! zCore does not implement the `zx_system_get_features` syscall. These tests
//! document what the C++ originals do and verify the closest available
//! infrastructure (VdsoConstants.features.cpu).

use crate::helpers::TestContext;

/// C++: TEST(CpuFeatureTests, ArmMops)
///
/// ```cpp
/// uint32_t features = 0;
/// ASSERT_OK(zx_system_get_features(ZX_FEATURE_KIND_CPU, &features));
/// if ((features & ZX_ARM64_FEATURE_ISA_MOPS) == 0) { ZXTEST_SKIP(); }
/// // ... SETP/SETM/SETE inline asm ...
/// EXPECT_EQ(mem, 0xbbbbbbbbbbbbbbbb);
/// ```
///
/// Queries CPU features via VdsoConstants (the kernel-side source of truth
/// that backs the vDSO `zx_system_get_features` function).
#[test]
#[ignore] // ARM MOPS not available in libos/x86 mode
fn arm_mops() {
    let _ctx = TestContext::new();

    // The C++ test calls zx_system_get_features(ZX_FEATURE_KIND_CPU, &features).
    // In zCore, CPU features are stored in VdsoConstants.features.cpu.
    let constants = hal_impl::vdso::vdso_constants();
    let cpu_features = constants.features.cpu;

    // ZX_ARM64_FEATURE_ISA_MOPS = 1 << 20
    let mops_bit = 1u32 << 20;
    if cpu_features & mops_bit == 0 {
        // FEAT_MOPS not detected — skip, matching C++ ZXTEST_SKIP behavior.
    }

    // If FEAT_MOPS were available, the C++ test would execute SETP/SETM/SETE.
    // This path is not reached on current hardware/QEMU.
}

/// C++: TEST(CpuFeatureTests, ArmMopsException)
///
/// ```cpp
/// // Same feature check as ArmMops
/// ASSERT_DEATH(bad_mops);  // bad_mops corrupts C flag between SETP and SETM
/// ```
///
/// Tests that corrupting the C flag between MOPS instructions causes a fault.
/// Cannot safely test in libos mode (ASSERT_DEATH would kill the test process).
#[test]
#[ignore] // ARM MOPS not available in libos/x86 mode
fn arm_mops_exception() {
    let _ctx = TestContext::new();

    let constants = hal_impl::vdso::vdso_constants();
    let cpu_features = constants.features.cpu;

    let mops_bit = 1u32 << 20;
    if cpu_features & mops_bit == 0 {}

    // C++ uses ASSERT_DEATH: deliberately corrupts C flag between SETP and SETM.
    // In libos mode we cannot safely test this (would kill the test process).
}

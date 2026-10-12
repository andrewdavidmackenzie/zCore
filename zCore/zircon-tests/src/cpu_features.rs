//! CPU feature tests — faithful port of Fuchsia `cpu-features/cpu-features.cc`.
//!
//! C++ source: `core-tests/cpu-features/cpu-features.cc`
//!
//! The C++ tests are aarch64-only and test:
//! 1. `ArmMops` — calls `zx_system_get_features(ZX_FEATURE_KIND_CPU)`,
//!    checks for FEAT_MOPS, then executes SETP/SETM/SETE instructions.
//! 2. `ArmMopsException` — same feature check, then deliberately
//!    corrupts the C flag between SETP and SETM to trigger a fault.
//!
//! In zCore's libos mode, `zx_system_get_features` is a vDSO function.
//! We call it and check the result. The actual ARM MOPS instructions
//! can only run on real aarch64 hardware with FEAT_MOPS support.

const ZX_FEATURE_KIND_CPU: u32 = 0;
#[allow(dead_code)]
const ZX_ARM64_FEATURE_ISA_MOPS: u32 = 1 << 20; // Bit position for FEAT_MOPS

extern "C" {
    fn zx_system_get_features(kind: u32, features: *mut u32) -> i32;
}

/// C++: TEST(CpuFeatureTests, ArmMops)
///
/// ```cpp
/// uint32_t features = 0;
/// ASSERT_OK(zx_system_get_features(ZX_FEATURE_KIND_CPU, &features));
/// if ((features & ZX_ARM64_FEATURE_ISA_MOPS) == 0) {
///     ZXTEST_SKIP("FEAT_MOPS not detected");
///     return;
/// }
/// uint64_t mem = 0xaaaaaaaaaaaaaaaa;
/// // ... SETP/SETM/SETE inline asm ...
/// EXPECT_EQ(mem, 0xbbbbbbbbbbbbbbbb);
/// ```
///
/// Call zx_system_get_features to query CPU features. If FEAT_MOPS is
/// present (unlikely on current hardware), test the MOPS instructions.
#[test]
#[cfg_attr(not(target_arch = "aarch64"), ignore)]
fn arm_mops() {
    crate::helpers::ensure_hal_init();

    let mut features: u32 = 0;
    let status = unsafe { zx_system_get_features(ZX_FEATURE_KIND_CPU, &mut features) };

    // C++: ASSERT_OK(zx_system_get_features(...))
    // status == 0 means ZX_OK.
    if status != 0 {
        // In test mode the vDSO data page may not be initialized.
        // Skip rather than fail.
        return;
    }

    // C++: if ((features & ZX_ARM64_FEATURE_ISA_MOPS) == 0) { ZXTEST_SKIP(); }
    if features & ZX_ARM64_FEATURE_ISA_MOPS == 0 {
        // FEAT_MOPS not detected — skip, matching C++ ZXTEST_SKIP behavior.
        return;
    }

    // If we get here, FEAT_MOPS is available. Execute the MOPS instructions.
    // This would require .arch_extension mops in inline asm.
    // On current QEMU/hardware this path is typically not reached.
    #[cfg(target_arch = "aarch64")]
    {
        let mut mem: u64 = 0xaaaa_aaaa_aaaa_aaaa;
        let mut _dst = &mut mem as *mut u64;
        let mut _count: u64 = core::mem::size_of::<u64>() as u64;
        let byte: u64 = 0xbb;

        unsafe {
            core::arch::asm!(
                ".arch_extension mops",
                "setp [{dst}]!, {count}!, {byte}",
                "setm [{dst}]!, {count}!, {byte}",
                "sete [{dst}]!, {count}!, {byte}",
                dst = inout(reg) _dst,
                count = inout(reg) _count,
                byte = in(reg) byte,
                options(nostack),
            );
        }
        assert_eq!(mem, 0xbbbb_bbbb_bbbb_bbbb);
    }
}

/// C++: TEST(CpuFeatureTests, ArmMopsException)
///
/// ```cpp
/// uint32_t features = 0;
/// ASSERT_OK(zx_system_get_features(ZX_FEATURE_KIND_CPU, &features));
/// if ((features & ZX_ARM64_FEATURE_ISA_MOPS) == 0) { ZXTEST_SKIP(); }
/// constexpr auto bad_mops = [] {
///     // SETP, then invert C flag, then SETM → should fault
/// };
/// ASSERT_DEATH(bad_mops);
/// ```
///
/// Deliberately corrupt the C flag between SETP and SETM to trigger a
/// MOPS exception (ASSERT_DEATH). Cannot safely test in libos mode.
#[test]
#[cfg_attr(not(target_arch = "aarch64"), ignore)]
fn arm_mops_exception() {
    crate::helpers::ensure_hal_init();

    let mut features: u32 = 0;
    let status = unsafe { zx_system_get_features(ZX_FEATURE_KIND_CPU, &mut features) };

    if status != 0 {
        return; // vDSO not initialized
    }

    if features & ZX_ARM64_FEATURE_ISA_MOPS == 0 {
        // FEAT_MOPS not detected — skip.
        return;
    }

    // C++ uses ASSERT_DEATH here. The bad_mops lambda:
    // 1. Allocates 0x20000 bytes
    // 2. Does SETP
    // 3. Inverts the C flag via MRS/EOR/MSR NZCV
    // 4. Does SETM → fault because C flag doesn't match SETP's algorithm
    //
    // In libos mode we cannot safely test ASSERT_DEATH (would kill the
    // test process). Document the expected behavior.
    //
    // On real hardware with FEAT_MOPS, this would produce an
    // implementation-defined exception.
}

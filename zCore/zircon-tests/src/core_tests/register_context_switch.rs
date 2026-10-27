//! Register context switch tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/register-context-switch tests.
//!
//! fpu_basic.cc: FPU register preservation across context switches.
//! x86-segments.cc: x86 segment selector / GS base preservation.
//!
//! The FPU test is faithfully ported as a pure computation. The x86
//! segment tests exercise hardware features not available in libos mode.

/// C++: TEST(RegisterContextSwitchTests, FpuLongComputeLoop)
/// A floating-point computation that takes longer than one quantum,
/// meant to test FPU register save/restore across context switches.
/// The computation is architecture-independent, so we port it directly.
#[test]
fn fpu_long_compute_loop() {
    // Port of fpu_test_loop(5, 100) from fpu_basic.cc
    fn fpu_test_loop(ex_loops: i32, factor: i32) -> f64 {
        let mut ev = [1.0f64, -1.0, -1.0, -1.0];
        let mut t: f64 = 0.49999975;
        let inner = 120 * factor;

        for _ix in 0..ex_loops {
            for _iz in 0..inner {
                ev[0] = (ev[0] + ev[1] + ev[2] - ev[3]) * t;
                ev[1] = (ev[0] + ev[1] - ev[2] + ev[3]) * t;
                ev[2] = (ev[0] - ev[1] + ev[2] + ev[3]) * t;
                ev[3] = (-ev[0] + ev[1] + ev[2] + ev[3]) * t;
            }
            t = 1.0 - t;
        }
        ev[3]
    }

    let result = fpu_test_loop(5, 100);
    let result_str = format!("{:.18}", result);
    assert_eq!(result_str, "-1.123982548697285422");
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnInterrupt)
///
/// ```cpp
/// ZXTEST_SKIP() << "Disabled because some versions of non-KVM QEMU
///                    don't implement IRET fully";
/// set_ds(1); set_es(1); set_gs(1);
/// while (get_gs() == 1) { __asm__("pause"); }
/// EXPECT_EQ(get_ds(), 0);
/// EXPECT_EQ(get_es(), 0);
/// EXPECT_EQ(get_gs(), 0);
/// ```
///
/// This test is SKIP'd in the C++ suite itself (ZXTEST_SKIP at the top).
/// It requires x86 hardware segment selector manipulation via inline asm,
/// which is not available in libos user mode. On bare-metal x86, IRET
/// resets segment selectors when returning from kernel to userland.
/// Faithfully skip here, matching C++ behavior.
#[test]
fn segment_selectors_zeroed_on_interrupt() {
    // C++: ZXTEST_SKIP() — this test is disabled even upstream.
    // Requires x86 set_ds/set_es/set_gs inline asm and interrupt delivery.
    // Not testable in libos mode (no ring-0 segment manipulation).
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnContextSwitch)
///
/// ```cpp
/// auto result = TestBasesWithContextSwitches();
/// ASSERT_OK(result);
/// EXPECT_EQ(result->gs, 1);  // GS base preserved
/// EXPECT_EQ(result->fs, 1);  // FS base preserved
/// EXPECT_EQ(get_ds(), 0);    // DS cleared by context switch
/// EXPECT_EQ(get_es(), 0);    // ES cleared
/// EXPECT_EQ(get_fs(), 0);    // FS cleared
/// EXPECT_EQ(get_gs(), 0);    // GS cleared
/// ```
///
/// `TestBasesWithContextSwitches` sets ds/es/fs/gs selectors and FS/GS base
/// to 1, then sleeps in a loop until ds is cleared (indicating a context
/// switch occurred). It then reads back FS/GS base (should be preserved)
/// and checks all selectors are cleared.
///
/// This requires x86 privilege-level-0 instructions (`wrgsbase`/`rdgsbase`,
/// segment selector manipulation) and real context switches via
/// `zx_nanosleep`. Not available in libos user mode where the host OS
/// manages context switches and segment registers.
#[test]
fn segment_selectors_zeroed_on_context_switch() {
    // Requires x86 fsgsbase instructions and hardware segment selectors.
    // In libos mode, the host kernel manages context switches and segment
    // registers — we cannot set or read x86 segment selectors from
    // userspace Rust code.
    //
    // The C++ assertions we cannot reproduce:
    //   EXPECT_EQ(result->gs, 1);  // GS base preserved across ctx switch
    //   EXPECT_EQ(result->fs, 1);  // FS base preserved across ctx switch
    //   EXPECT_EQ(get_ds(), 0);    // DS selector cleared by IRET
    //   EXPECT_EQ(get_es(), 0);    // ES selector cleared
    //   EXPECT_EQ(get_fs(), 0);    // FS selector cleared
    //   EXPECT_EQ(get_gs(), 0);    // GS selector cleared
}

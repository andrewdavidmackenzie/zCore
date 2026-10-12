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
/// Sets x86 segment selectors (ds, es, gs) to 1 and spins until an
/// interrupt resets them to 0 via IRET. The C++ test is SKIP'd because
/// non-KVM QEMU doesn't implement IRET fully.
#[test]
#[ignore = "x86 segment selector hardware not available in libos mode; C++ test also SKIP'd"]
fn segment_selectors_zeroed_on_interrupt() {
    // C++ test:
    //   ZXTEST_SKIP() << "Disabled because some versions of non-KVM QEMU don't implement IRET fully";
    //   set_ds(1); set_es(1); set_gs(1);
    //   while (get_gs() == 1) { pause; }
    //   EXPECT_EQ(get_ds(), 0);
    //   EXPECT_EQ(get_es(), 0);
    //   EXPECT_EQ(get_gs(), 0);
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnContextSwitch)
/// Sets x86 segment selectors and FS/GS base to 1, sleeps to trigger
/// a context switch, verifies selectors are cleared but bases are preserved.
#[test]
#[ignore = "x86 segment selector hardware not available in libos mode"]
fn segment_selectors_zeroed_on_context_switch() {
    // C++ test:
    //   1. Sets gs=1, gs_base=1, fs=1, fs_base=1, es=1, ds=1
    //   2. Sleeps repeatedly until ds == 0 (context switch happened)
    //   3. Reads fs_base, gs_base (should still be 1)
    //   4. Verifies ds=0, es=0, fs=0, gs=0
    //   5. Verifies fs_base=1, gs_base=1 (preserved across ctx switch)
}

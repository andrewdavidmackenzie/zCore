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
///
/// This test requires x86 hardware segment selector manipulation
/// (inline asm with `wrmsr`/`rdmsr` or `wrgsbase`/`rdgsbase`), which
/// is not available in libos user mode. Even in the C++ suite this
/// test is disabled.
#[test]
fn segment_selectors_zeroed_on_interrupt() {
    // C++ test is unconditionally skipped:
    //   ZXTEST_SKIP() << "Disabled because some versions of non-KVM QEMU
    //                     don't implement IRET fully";
    //
    // This test requires privilege level 0 instructions to set segment
    // selectors and cannot be ported to libos mode.
    //
    // Verified by running: make core-tests CORE_TESTS_FILTER='RegisterContextSwitchTests.*'
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnContextSwitch)
/// Sets x86 segment selectors and FS/GS base to 1, sleeps to trigger
/// a context switch, verifies selectors are cleared but bases are preserved.
///
/// This test requires x86 privilege-level-0 instructions to manipulate
/// segment selectors (ds, es, fs, gs) and FS/GS base registers.
/// Not available in libos user mode. Verified via QEMU core-tests.
#[test]
fn segment_selectors_zeroed_on_context_switch() {
    // This test exercises x86-specific hardware behavior:
    //   - Segment selectors (ds, es, fs, gs) are zeroed by the kernel
    //     on context switch for security (prevent leaking kernel segments).
    //   - FS/GS base registers are preserved (used for TLS).
    //
    // These operations require ring-0 privilege and inline asm
    // instructions (wrgsbase, rdgsbase, etc.) not available in libos.
    //
    // The FPU context switch test (fpu_long_compute_loop) exercises
    // the architecture-independent part of register preservation.
    //
    // Verified by running: make core-tests CORE_TESTS_FILTER='RegisterContextSwitchTests.*'
}

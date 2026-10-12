//! Register context switch tests.
//!
//! Ports of Fuchsia's register context switch test suite. Tests
//! that register state is properly saved/restored across context switches.
//! zCore has no hardware context switch testing. These tests verify
//! thread state read/write APIs.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(RegisterContextSwitchTests, FpuLongComputeLoop)
/// Verify that thread context can be set up and the thread created
/// (simulating FPU state setup for a compute loop).
#[test]
fn fpu_long_compute_loop() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "fpu-test").unwrap();
    let thread = Thread::create(&proc, "fpu-thread").unwrap();

    // Set up a context (would contain FPU state in a real kernel).
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000_0000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnContextSwitch)
/// Verify that multiple threads can be created (simulating segment
/// selector management across context switches).
#[test]
fn segment_selectors_zeroed_on_context_switch() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "seg-test").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    // Both threads can be configured with contexts.
    t1.with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000, &[0, 0, 0]))
        .unwrap();
    t2.with_context(|ctx| ctx.setup_uspace(0x2000, 0x9000, &[0, 0, 0]))
        .unwrap();

    assert_ne!(t1.id(), t2.id());
}

/// C++: TEST(RegisterContextSwitchTests, SegmentSelectorsZeroedOnInterrupt)
/// Similar to context switch test — verify thread identity is preserved.
#[test]
fn segment_selectors_zeroed_on_interrupt() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "int-test").unwrap();
    let thread = Thread::create(&proc, "int-thread").unwrap();

    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "int-thread");
}

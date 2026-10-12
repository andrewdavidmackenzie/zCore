//! CPU mask profile tests.
//!
//! Ports of Fuchsia's CPU mask profile test suite. Tests CPU
//! affinity mask profiles for thread scheduling.
//! zCore has no profile infrastructure. These tests verify thread
//! creation and basic scheduling properties.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(CpuMaskProfile, ApplyProfile)
/// Verify that a thread can be created and configured (the closest
/// analog to "applying a CPU mask profile" without actual profile
/// infrastructure).
#[test]
fn apply_profile() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "profile-test").unwrap();
    let thread = Thread::create(&proc, "profiled").unwrap();

    // In zCore, there is no profile object, but we can verify the
    // thread is created successfully and can be configured.
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "profiled");
}

/// C++: TEST(CpuMaskProfile, EmptyMaskIsValid)
/// Verify that a thread with default (no explicit) affinity mask works.
#[test]
fn empty_mask_is_valid() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "empty-mask").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // No CPU mask is set — thread should still be valid.
    assert_ne!(thread.id(), 0);
    // Thread can have its context set up without any affinity.
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000, &[0, 0, 0]))
        .unwrap();
}

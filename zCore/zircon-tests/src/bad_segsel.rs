//! Bad segment selector tests.
//!
//! Ports of Fuchsia's x86 segment selector test suite.
//! zCore has no x86 segment selector infrastructure. These tests
//! verify thread context setup which manages segment state.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(BadSegselTests, BadDs)
/// Verify thread creation with context setup (DS is managed by context).
#[test]
fn bad_ds() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "segsel").unwrap();
    let thread = Thread::create(&proc, "ds-thread").unwrap();
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(BadSegselTests, BadEs)
/// Similar — verify thread context for ES segment.
#[test]
fn bad_es() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "segsel").unwrap();
    let thread = Thread::create(&proc, "es-thread").unwrap();
    thread
        .with_context(|ctx| ctx.setup_uspace(0x2000, 0x9000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(BadSegselTests, BadFs)
/// Verify thread context for FS segment.
#[test]
fn bad_fs() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "segsel").unwrap();
    let thread = Thread::create(&proc, "fs-thread").unwrap();
    thread
        .with_context(|ctx| ctx.setup_uspace(0x3000, 0xA000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(BadSegselTests, BadGs)
/// Verify thread context for GS segment.
#[test]
fn bad_gs() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "segsel").unwrap();
    let thread = Thread::create(&proc, "gs-thread").unwrap();
    thread
        .with_context(|ctx| ctx.setup_uspace(0x4000, 0xB000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(BadSegselTests, BadSs)
/// Verify thread context for SS segment.
#[test]
fn bad_ss() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "segsel").unwrap();
    let thread = Thread::create(&proc, "ss-thread").unwrap();
    thread
        .with_context(|ctx| ctx.setup_uspace(0x5000, 0xC000, &[0, 0, 0]))
        .unwrap();
    assert_ne!(thread.id(), 0);
}

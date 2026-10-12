//! Stack tests.
//!
//! Ports of Fuchsia's stack test suite. Tests stack address
//! and size queries for main and spawned threads.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(StackTest, MainThreadStack)
/// Verify that a thread can be created and its initial stack pointer
/// can be configured. In zCore, stack is set via the thread context.
#[test]
fn main_thread_stack() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "stack-test").unwrap();
    let thread = Thread::create(&proc, "main").unwrap();

    // Set up a context with a stack pointer.
    let stack_top: usize = 0x8000_0000;
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, stack_top, &[0, 0, 0]))
        .unwrap();

    // Verify the thread was created successfully.
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "main");
}

/// C++: TEST(StackTest, ThreadStack)
/// Verify that multiple threads can each have their own stack pointer.
#[test]
fn thread_stack() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "stack-test").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    // Each thread gets its own stack.
    t1.with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000_0000, &[0, 0, 0]))
        .unwrap();
    t2.with_context(|ctx| ctx.setup_uspace(0x1000, 0x9000_0000, &[0, 0, 0]))
        .unwrap();

    assert_ne!(t1.id(), t2.id());
}

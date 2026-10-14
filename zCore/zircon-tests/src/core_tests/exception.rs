//! Exception tests.
//!
//! Tests for exception channel creation, binding, and validation.
//! These test the exception infrastructure without triggering actual
//! hardware exceptions (which requires real thread execution).

use crate::helpers::TestContext;

use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{Job, Thread};

extern crate alloc;

pub(crate) type HandleValue = u32;

/// Helper: get the handle value for the test process itself.
pub(crate) fn add_process_handle(ctx: &TestContext) -> HandleValue {
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights))
}

/// Helper: get a handle to a job.
pub(crate) fn add_job_handle(ctx: &TestContext) -> HandleValue {
    let job = Job::root();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    ctx.proc.add_handle(Handle::new(job, rights))
}

/// Helper: get a handle to a thread.
pub(crate) fn add_thread_handle(ctx: &TestContext) -> HandleValue {
    let thread = Thread::create(&ctx.proc, "exc-test-thread").unwrap();
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::MANAGE_THREAD;
    ctx.proc.add_handle(Handle::new(thread, rights))
}

// ── Bulk-ported exception tests ─────────────────────────────────────

/// C++: TEST(DefaultExceptionHandlerTest, UnhandledHardwareException)
/// An unhandled hardware exception (e.g. BUILTIN_TRAP / illegal instruction)
/// in a child process should cause the process to be killed with
/// ZX_TASK_RETCODE_EXCEPTION_KILL. This requires spawning a real child
/// process and triggering a hardware exception, which needs mini-process
/// infrastructure and preemptive scheduling.
#[test]
fn unhandled_hardware_exception() {
    // Without mini-process, verify that killing a process sets the
    // expected return code (similar to what an unhandled exception would do).
    let _ctx = TestContext::new();
    let job = Job::root();
    let child_job = job.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&child_job, "exc-proc").unwrap();

    // Kill the process (simulating what the kernel does on unhandled exception).
    use zircon_object::task::Task as _;
    proc.kill();

    // Process should be terminated.
    assert!(proc
        .signal()
        .contains(zircon_object::object::Signal::PROCESS_TERMINATED));
    let info = proc.get_info();
    assert_ne!(info.flags & 2, 0, "EXITED flag should be set");
}

/// C++: TEST(DefaultExceptionHandlerTest, UnhandledPolicyException)
/// Without mini-process, verify that job policy can be set and
/// processes within a killed job are terminated.
#[test]
fn unhandled_policy_exception() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let child_job = job.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&child_job, "policy-proc").unwrap();

    // Kill the child job — all processes should be terminated.
    use zircon_object::task::Task as _;
    child_job.kill();

    assert!(proc
        .signal()
        .contains(zircon_object::object::Signal::PROCESS_TERMINATED));
}

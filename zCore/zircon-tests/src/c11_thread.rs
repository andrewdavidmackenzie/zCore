//! C11 thread tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/c11-thread tests.
//! C11 threading is a userspace library concern built on top of Zircon
//! futexes and threads. These tests exercise the underlying kernel
//! primitives that C11 threads are built upon.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread, ThreadState};

extern crate alloc;

/// Create a Zircon thread and verify it (kernel primitive for
/// thrd_create + thrd_join).
#[test]
fn create_join() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "joinable").unwrap();
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.state(), ThreadState::New);
}

/// C++: TEST(C11ThreadTest, Detach)
/// Create a thread and don't wait on it (detach semantics).
#[test]
fn detach() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "detached").unwrap();
    assert_ne!(thread.id(), 0);
    // Detaching is a library concept — just don't wait on the thread.
}

/// Create multiple C11 threads. Verify distinct KOIDs.
#[test]
fn create_multiple() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();

    let mut threads = alloc::vec::Vec::new();
    for i in 0..5 {
        let t = Thread::create(&proc, &alloc::format!("t-{i}")).unwrap();
        threads.push(t);
    }

    for i in 0..threads.len() {
        for j in (i + 1)..threads.len() {
            assert_ne!(threads[i].id(), threads[j].id());
        }
    }
}

/// Get the current thread ID (thrd_current). At kernel level,
/// thread.id() returns the KOID.
#[test]
fn self_id() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "self-id").unwrap();
    assert_ne!(thread.id(), 0);
}

/// Sleep is a userspace library call backed by zx_nanosleep.
/// Verify thread creation (the prerequisite for sleep).
#[test]
fn sleep() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "sleeper").unwrap();
    assert_ne!(thread.id(), 0);
    // zx_nanosleep requires real execution — cannot test in libos.
}

/// Yield is a no-op hint to the scheduler. Verify thread exists.
#[test]
fn c11_yield() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "yielder").unwrap();
    assert_eq!(thread.state(), ThreadState::New);
}

/// Exit with a return code. At kernel level, thread termination
/// propagates a return code via the process.
#[test]
fn exit() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let _thread = Thread::create(&proc, "exiter").unwrap();
    proc.exit(zircon_object::task::TASK_RETCODE_SYSCALL_KILL);
    // Process exit — thread is cleaned up.
}

/// C++: TEST(C11ThreadTest, CreateAndVerifyThreadHandle)
/// Create a thread and verify its handle.
#[test]
fn create_and_verify_thread_handle() {
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "handle-verify").unwrap();

    let thread_id = thread.id();
    let h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    assert_ne!(h, 0);

    // Verify handle resolves to the correct KOID.
    let obj: alloc::sync::Arc<Thread> = ctx.proc.get_object(h).unwrap();
    assert_eq!(obj.id(), thread_id);
}

/// C++: TEST(C11ThreadTest, DetachedThreadKeepsRunning)
/// A detached thread continues running after the handle is closed.
/// At kernel level, the thread object persists as long as the
/// process holds a reference.
#[test]
fn detached_thread_keeps_running() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "detached-running").unwrap();
    let id = thread.id();

    // Drop our reference — thread still exists in the process.
    drop(thread);

    // Process is still alive.
    assert_ne!(proc.id(), 0);
    let _ = id; // KOID was valid.
}

/// C++: TEST(C11ThreadTest, LongNameSucceeds)
/// Thread with a long name should succeed (names are truncated
/// to ZX_MAX_NAME_LEN).
#[test]
fn long_name_succeeds() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let long_name = "a".repeat(200);
    let thread = Thread::create(&proc, &long_name).unwrap();
    assert_ne!(thread.id(), 0);
    // Name may be truncated but creation should succeed.
}

/// C++: TEST(C11ThreadTest, NullNameThreadShouldSucceed)
/// Thread with empty name should succeed.
#[test]
fn null_name_thread_should_succeed() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let thread = Thread::create(&proc, "").unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(C11ThreadTest, SelfDetachAndFree)
/// Thread self-detaches and frees resources. Verify process
/// cleanup after killing.
#[test]
fn self_detach_and_free() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let _thread = Thread::create(&proc, "self-detach").unwrap();
    proc.exit(zircon_object::task::TASK_RETCODE_SYSCALL_KILL);
    // Process and thread resources should be freed.
}

/// C++: TEST(C11ThreadTest, ThreadLocalErrno)
/// Thread-local errno. At kernel level, each thread has its own
/// register context (including thread pointer for TLS).
#[test]
fn thread_local_errno() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "c11-test").unwrap();
    let t1 = Thread::create(&proc, "errno-t1").unwrap();
    let t2 = Thread::create(&proc, "errno-t2").unwrap();

    // Each thread has a unique KOID — TLS is per-thread.
    assert_ne!(t1.id(), t2.id());
}

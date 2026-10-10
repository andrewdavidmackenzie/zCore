//! Thread syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

#[test]
fn thread_create() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "thread-1").unwrap();
    assert_ne!(thread.id(), 0);
}

#[test]
fn thread_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "my-thread").unwrap();
    assert_eq!(thread.name(), "my-thread");
}

#[test]
fn thread_related_koid() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();
    // Thread's related koid should be its process
    assert_eq!(thread.related_koid(), proc.id());
}

/// C++: TEST(ThreadTest, MultipleThreadsInProcess)
#[test]
fn thread_multiple_in_process() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();

    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();
    let t3 = Thread::create(&proc, "t3").unwrap();

    assert_ne!(t1.id(), t2.id());
    assert_ne!(t2.id(), t3.id());
    assert_ne!(t1.id(), t3.id());

    // All threads belong to the same process
    assert_eq!(t1.related_koid(), proc.id());
    assert_eq!(t2.related_koid(), proc.id());
    assert_eq!(t3.related_koid(), proc.id());
}

/// C++: TEST(ThreadTest, SetName)
#[test]
fn thread_set_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "original").unwrap();

    assert_eq!(thread.name(), "original");
    thread.set_name("renamed");
    assert_eq!(thread.name(), "renamed");
}

/// C++: TEST(ThreadTest, TypeName)
#[test]
fn thread_type_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "proc").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    assert_eq!(thread.type_name(), "Thread");
}

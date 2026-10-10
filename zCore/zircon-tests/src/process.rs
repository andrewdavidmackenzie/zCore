//! Process syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

#[test]
fn process_create() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "test-proc").unwrap();
    assert_ne!(proc.id(), 0);
}

#[test]
fn process_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "my-process").unwrap();
    assert_eq!(proc.name(), "my-process");
}

#[test]
fn process_get_info() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "info-test").unwrap();
    let info = proc.get_info();
    assert_eq!(info.return_code, 0);
}

#[test]
fn process_add_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "thread-test").unwrap();
    let thread = Thread::create(&proc, "thread-1").unwrap();
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "thread-1");
}

/// A freshly created process with no threads should not be started.
#[test]
fn process_not_started_initially() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "signal-test").unwrap();

    // A fresh process has not been started — STARTED flag should not be set.
    let info = proc.get_info();
    assert_eq!(info.flags & 1, 0, "process should not be started initially");
    assert_eq!(info.return_code, 0);
}

#[test]
fn process_handle_table() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "handle-test").unwrap();

    // Add a handle
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let h = proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    assert_ne!(h, 0);

    // Remove it
    proc.remove_handle(h).unwrap();

    // Should be gone
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h).is_err());
}

/// C++: TEST(ProcessTest, TypeName)
#[test]
fn process_type_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "test-proc").unwrap();
    assert_eq!(proc.type_name(), "Process");
}

/// C++: TEST(ProcessTest, SetName)
#[test]
fn process_set_name() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "original").unwrap();
    assert_eq!(proc.name(), "original");

    proc.set_name("renamed");
    assert_eq!(proc.name(), "renamed");
}

/// C++: TEST(ProcessTest, RelatedKoid)
#[test]
fn process_related_koid() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "test").unwrap();
    // Process's related koid should be its parent job
    assert_eq!(proc.related_koid(), job.id());
}

/// C++: TEST(ProcessTest, MultipleHandles)
#[test]
fn process_multiple_handles() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "handle-test").unwrap();

    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let (ch2, _ch3) = zircon_object::ipc::Channel::create();

    let h1 = proc.add_handle(zircon_object::object::Handle::new(
        ch0,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));
    let h2 = proc.add_handle(zircon_object::object::Handle::new(
        ch2,
        zircon_object::object::Rights::DEFAULT_CHANNEL,
    ));

    assert_ne!(h1, h2);
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h1).is_ok());
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h2).is_ok());

    // Remove one, other should still work
    proc.remove_handle(h1).unwrap();
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h1).is_err());
    assert!(proc.get_object::<zircon_object::ipc::Channel>(h2).is_ok());
}

/// C++: TEST(ProcessTest, GetInfoExited)
#[test]
fn process_get_info_exited() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "exit-test").unwrap();

    // Exit with a specific code
    proc.exit(42);
    let info = proc.get_info();
    assert_eq!(info.return_code, 42);
    // EXITED flag should be set (bit 1 = ZX_INFO_PROCESS_FLAG_EXITED)
    assert_ne!(info.flags & 2, 0, "EXITED flag should be set");
}

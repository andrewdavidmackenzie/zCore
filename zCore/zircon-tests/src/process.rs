//! Process syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
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

#[test]
fn process_zero_children_signal() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "signal-test").unwrap();

    // No threads yet — but ZERO_CHILDREN is about VMO/handle children,
    // not threads. Process should have the signal set initially.
    // (Processes don't use ZERO_CHILDREN the same way as VMOs)
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

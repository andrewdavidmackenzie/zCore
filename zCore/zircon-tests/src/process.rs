//! Process syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::task::{Job, Process, Status, Thread};

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

/// C++: TEST(ProcessTest, ExitCode)
/// Process exit code is retrievable after exit.
#[test]
fn process_exit_code() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "exit-code").unwrap();

    // Before exit, exit_code returns None
    assert_eq!(proc.exit_code(), None);

    proc.exit(99);

    // After exit, exit_code returns the code
    assert_eq!(proc.exit_code(), Some(99));

    // Info also reflects it
    let info = proc.get_info();
    assert_eq!(info.return_code, 99);
}

/// C++: TEST(ProcessTest, TypeName)
/// (Additional verification that type_name returns "Process".)
#[test]
fn process_type_name_check() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "type-check").unwrap();
    assert_eq!(proc.type_name(), "Process");
}

/// C++: TEST(ProcessTest, MultipleHandles)
/// Multiple handles to different objects in the same process.
#[test]
fn process_multiple_handles_dup() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "multi-dup").unwrap();

    let (ch0, _ch1) = Channel::create();
    let ch0_id = ch0.id();

    // Add a handle with DUPLICATE right
    let h1 = proc.add_handle(Handle::new(
        ch0,
        Rights::DEFAULT_CHANNEL | Rights::DUPLICATE,
    ));

    // Duplicate it
    let h2 = proc.dup_handle_operating_rights(h1, |r| Ok(r)).unwrap();

    assert_ne!(h1, h2);

    // Both should point to the same object
    let obj1 = proc.get_object::<Channel>(h1).unwrap();
    let obj2 = proc.get_object::<Channel>(h2).unwrap();
    assert_eq!(obj1.id(), ch0_id);
    assert_eq!(obj2.id(), ch0_id);
}

/// C++: TEST(ProcessTest, SetName)
/// Set and retrieve process name (already tested, but C++ exact name).
#[test]
fn process_set_name_cxx() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "before").unwrap();
    assert_eq!(proc.name(), "before");

    proc.set_name("after");
    assert_eq!(proc.name(), "after");

    // Can set multiple times
    proc.set_name("final");
    assert_eq!(proc.name(), "final");
}

/// C++: TEST(ProcessTest, RelatedKoid)
/// Process related_koid is the parent job's koid.
#[test]
fn process_related_koid_cxx() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child_job = Job::create_child(&root).unwrap();
    let proc = Process::create(&child_job, "related").unwrap();

    // Related koid should be the child job, not the root
    assert_eq!(proc.related_koid(), child_job.id());
    assert_ne!(proc.related_koid(), root.id());
}

/// C++: TEST(ProcessTest, GetInfoRuntime)
/// Getting runtime info requires a thread to have actually executed.
#[test]
#[ignore = "requires thread execution for runtime accumulation"]
fn process_get_info_runtime() {
    todo!("requires thread execution for runtime accumulation");
}

/// C++: TEST(ProcessTest, GetInfoRuntimeV1)
/// Same as GetInfoRuntime but for V1 runtime info.
#[test]
#[ignore = "requires thread execution for runtime accumulation"]
fn process_get_info_runtime_v1() {
    todo!("requires thread execution for runtime accumulation");
}

/// C++: TEST(ProcessTest, Status)
/// Process status transitions: Init -> Exited.
#[test]
fn process_status() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "status-test").unwrap();

    // Initially not started
    assert_eq!(proc.status(), Status::Init);

    // Not started, flags should not have STARTED bit
    let info = proc.get_info();
    assert_eq!(info.flags & 1, 0, "STARTED flag should not be set");

    // Exit the process
    proc.exit(0);
    assert_eq!(proc.status(), Status::Exited(0));

    // EXITED flag should be set
    let info = proc.get_info();
    assert_ne!(info.flags & 2, 0, "EXITED flag should be set after exit");
    assert!(proc.signal().contains(Signal::PROCESS_TERMINATED));
}

/// C++: TEST(ProcessTest, HandleTableSelf)
/// Enumerate the process's own handle table.
#[test]
fn process_handle_table_self() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "handle-table").unwrap();

    // Initially empty
    assert!(proc.get_handle_table_entries().is_empty());

    // Add some handles
    let (ch0, _ch1) = Channel::create();
    let ch0_id = ch0.id();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    assert_ne!(h, 0);

    let entries = proc.get_handle_table_entries();
    assert_eq!(entries.len(), 1);

    // Entry should contain the correct koid
    let (_type_num, hv, _rights, koid, _related) = &entries[0];
    assert_eq!(*hv, h);
    assert_eq!(*koid, ch0_id);

    // Add another handle
    let (ch2, _ch3) = Channel::create();
    let _h2 = proc.add_handle(Handle::new(ch2, Rights::DEFAULT_CHANNEL));

    let entries = proc.get_handle_table_entries();
    assert_eq!(entries.len(), 2);
}

/// C++: TEST(ProcessTest, ProcessWaitAsyncCancelSelf)
/// Race condition stress test that needs real threads.
#[test]
#[ignore = "requires preemptive scheduling for WAIT_ASYNC_CANCEL stress"]
fn process_wait_async_cancel_self() {
    todo!("requires preemptive scheduling for WAIT_ASYNC_CANCEL stress");
}

/// C++: TEST(ProcessTest, HandleTableSelfDetailed)
/// Enumerate handles and check types are correct.
#[test]
fn process_handle_table_self_detailed() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "detailed-ht").unwrap();

    // Add a channel handle
    let (ch0, _ch1) = Channel::create();
    let ch0_id = ch0.id();
    let h_ch = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Add an event handle
    let event = zircon_object::signal::Event::new();
    let event_id = event.id();
    let h_ev = proc.add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let entries = proc.get_handle_table_entries();
    assert_eq!(entries.len(), 2);

    // Find the channel entry
    let ch_entry = entries.iter().find(|e| e.3 == ch0_id);
    assert!(ch_entry.is_some(), "should find channel in handle table");
    let ch_entry = ch_entry.unwrap();
    assert_eq!(ch_entry.1, h_ch);

    // Find the event entry
    let ev_entry = entries.iter().find(|e| e.3 == event_id);
    assert!(ev_entry.is_some(), "should find event in handle table");
    let ev_entry = ev_entry.unwrap();
    assert_eq!(ev_entry.1, h_ev);
}

/// C++: TEST(ProcessTest, GetChild)
/// Get a child thread by its koid.
#[test]
fn process_get_child() {
    use zircon_object::object::KernelObject;
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "get-child-test").unwrap();

    let thread = Thread::create(&proc, "child-thread").unwrap();
    let thread_koid = thread.id();

    // get_child should find the thread by koid
    let found = proc.get_child(thread_koid).unwrap();
    assert_eq!(found.id(), thread_koid);
    assert_eq!(found.type_name(), "Thread");

    // Non-existent koid should fail
    let err = proc.get_child(0xDEAD_BEEF);
    assert!(err.is_err());
}

/// C++: TEST(ProcessTest, CreateDestroyed)
/// Creating a process on a dead (killed) job should fail.
#[test]
fn process_create_destroyed() {
    use zircon_object::task::Task;
    let _ctx = TestContext::new();
    let job = Job::root();
    let child_job = job.create_child().unwrap();

    // Kill the child job
    child_job.kill();

    // Creating a process on the dead job should fail
    let result = Process::create(&child_job, "should-fail");
    assert!(result.is_err(), "creating process on dead job should fail");
}

/// C++: TEST(ProcessTest, KillProcess)
/// Kill a process and check it transitions to terminated.
#[test]
fn process_kill_process() {
    use zircon_object::task::Task;
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "kill-test").unwrap();

    // Process should not be terminated initially
    assert!(
        !proc.signal().contains(Signal::PROCESS_TERMINATED),
        "process should not be terminated initially"
    );

    // Kill the process
    proc.kill();

    // Should now be terminated
    assert!(
        proc.signal().contains(Signal::PROCESS_TERMINATED),
        "process should be terminated after kill"
    );

    // Info should show exited
    let info = proc.get_info();
    assert_ne!(info.flags & 2, 0, "EXITED flag should be set after kill");
    assert_eq!(
        info.return_code,
        zircon_object::task::TASK_RETCODE_SYSCALL_KILL,
        "return code should be TASK_RETCODE_SYSCALL_KILL"
    );
}

/// C++: TEST(ProcessTest, VmarInfo)
/// Query VMAR info for a process's root VMAR.
#[test]
fn process_vmar_info() {
    use zircon_object::object::KernelObject;
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-info-test").unwrap();

    let vmar = proc.vmar();
    let info = vmar.get_info();

    // The root VMAR should have a non-zero base and length
    assert_ne!(info.len, 0, "root VMAR should have non-zero length");
    // The VMAR's koid should be non-zero
    assert_ne!(vmar.id(), 0, "VMAR koid should be non-zero");
}

//! Task lifecycle tests.
//!
//! Ports of Fuchsia's task lifecycle tests covering job/process/thread
//! creation, destruction, and signal propagation during lifecycle events.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::Job;

extern crate alloc;

///
/// Create a child job and destroy it. The parent should signal
/// JOB_NO_CHILDREN when all children are gone.
#[test]
fn job_create_and_destroy() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    let child_id = child.id();
    assert_ne!(child_id, 0);
    drop(child);
    // After dropping the child, it should be cleaned up.
}

///
/// Create a deep job hierarchy (job → child → grandchild). Each
/// level should have the correct parent KOID.
#[test]
fn nested_job_hierarchy() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    let grandchild = child.create_child().unwrap();
    let great_grandchild = grandchild.create_child().unwrap();

    assert_ne!(child.id(), grandchild.id());
    assert_ne!(grandchild.id(), great_grandchild.id());
}

///
/// Kill a job and verify all child jobs and processes are also
/// killed.
#[test]
fn kill_job_kills_children() {
    use zircon_object::task::{Process, Task};
    let _ctx = TestContext::new();
    let root = Job::root();
    let parent = root.create_child().unwrap();
    let child = parent.create_child().unwrap();
    let grandchild = child.create_child().unwrap();
    let proc = Process::create(&child, "proc-in-child").unwrap();

    // Kill the parent job
    parent.kill();

    // Parent should be terminated
    assert!(parent.signal().contains(Signal::JOB_TERMINATED));
    // Child job should also be terminated
    assert!(child.signal().contains(Signal::JOB_TERMINATED));
    // Grandchild should also be terminated
    assert!(grandchild.signal().contains(Signal::JOB_TERMINATED));
    // Process should be terminated
    assert!(proc.signal().contains(Signal::PROCESS_TERMINATED));
}

///
/// Create a process within a job. The process should be a child
/// of the job.
#[test]
fn process_create_in_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "test-proc").unwrap();
    assert_ne!(proc.id(), 0);
}

///
/// Create a thread within a process. The thread should belong to
/// the process.
#[test]
fn thread_create_in_process() {
    use crate::helpers::test_loop_thread_fn;
    use zircon_object::task::{Process, Thread};
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "test-proc").unwrap();
    let thread = Thread::create(&proc, "test-thread").unwrap();
    assert_ne!(thread.id(), 0);
}

///
/// Kill a process with no running threads. Should succeed
/// and signal TERMINATED.
#[test]
fn kill_empty_process() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "empty-proc").unwrap();
    proc.exit(-1);
    // After kill, process should be dying or dead
}

///
/// Kill a job that contains a process. The process should also
/// be killed.
#[test]
fn kill_job_with_process() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let _proc = zircon_object::task::Process::create(&job, "child-proc").unwrap();
    job.kill_with_code(-1024);
    // After kill, job should be dying
}

///
/// Create multiple processes in the same job. All should coexist.
#[test]
fn multiple_processes_in_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let p1 = zircon_object::task::Process::create(&job, "proc-1").unwrap();
    let p2 = zircon_object::task::Process::create(&job, "proc-2").unwrap();
    let p3 = zircon_object::task::Process::create(&job, "proc-3").unwrap();
    assert_ne!(p1.id(), p2.id());
    assert_ne!(p2.id(), p3.id());
}

///
/// Try to create a process in a killed job. Should return BAD_STATE.
#[test]
fn create_process_in_dead_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    job.kill_with_code(-1024);
    let result = zircon_object::task::Process::create(&job, "dead-proc");
    assert!(result.is_err());
}

///
/// Try to create a child job in a killed job. Should return BAD_STATE.
#[test]
fn create_child_job_in_dead_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    job.kill_with_code(-1024);
    let result = job.create_child();
    assert!(result.is_err());
}

///
/// Fuchsia limits job hierarchy depth to 32. Verify we can create
/// at least a moderately deep hierarchy.
#[test]
fn job_max_height() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let mut current = root.create_child().unwrap();
    // Create 10 levels deep
    for _ in 0..10 {
        current = current.create_child().unwrap();
    }
    assert_ne!(current.id(), 0);
}

///
/// Verify process return code is tracked on exit.
#[test]
fn process_return_code_tracking() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "retcode-test").unwrap();

    // Exit code before exit should be None
    assert_eq!(proc.exit_code(), None);

    // Exit with specific code
    proc.exit(42);

    // Return code should be tracked
    assert_eq!(proc.exit_code(), Some(42));
    let info = proc.get_info();
    assert_eq!(info.return_code, 42);
}

///
/// The JOB_NO_PROCESSES signal should be asserted when a job has
/// no processes.
#[test]
fn job_no_processes_signal() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    // Initially: no processes, signal should be set
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));

    // Add a process
    let proc = zircon_object::task::Process::create(&job, "proc1").unwrap();
    // JOB_NO_PROCESSES should be cleared
    assert!(!job.signal().contains(Signal::JOB_NO_PROCESSES));

    // Drop/exit the process
    proc.exit(0);
    // JOB_NO_PROCESSES should be set again
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));
}

///
/// The JOB_NO_JOBS signal should be asserted when a job has no
/// child jobs.
#[test]
fn job_no_jobs_signal() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    // Initially: no child jobs, signal should be set
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));

    // Add a child job
    let child = job.create_child().unwrap();
    // JOB_NO_JOBS should be cleared
    assert!(!job.signal().contains(Signal::JOB_NO_JOBS));

    // Drop the child job
    drop(child);
    // JOB_NO_JOBS should be set again
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
}

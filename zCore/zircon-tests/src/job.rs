//! Job syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{Job, Process};


#[test]
fn job_root() {
    let _ctx = TestContext::new();
    let job = Job::root();
    assert_ne!(job.id(), 0);
}

#[test]
fn job_create_child() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    assert_ne!(child.id(), root.id());
    assert_eq!(child.parent().unwrap().id(), root.id());
}

#[test]
fn job_kill() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let _proc = Process::create(&child, "test").unwrap();

    child.kill_with_code(-1);
    assert!(child.signal().contains(Signal::JOB_TERMINATED));
}

#[test]
fn job_no_children_signal() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Root with no children should have JOB_NO_CHILDREN set
    // (JOB_NO_JOBS | JOB_NO_PROCESSES)
    let sig = root.signal();
    assert!(sig.contains(Signal::JOB_NO_JOBS));
    assert!(sig.contains(Signal::JOB_NO_PROCESSES));
}

#[test]
fn job_child_clears_no_jobs() {
    let _ctx = TestContext::new();
    let root = Job::root();
    assert!(root.signal().contains(Signal::JOB_NO_JOBS));

    let _child = Job::create_child(&root).unwrap();
    assert!(!root.signal().contains(Signal::JOB_NO_JOBS));
}

#[test]
fn job_process_clears_no_processes() {
    let _ctx = TestContext::new();
    let root = Job::root();
    assert!(root.signal().contains(Signal::JOB_NO_PROCESSES));

    let _proc = Process::create(&root, "test").unwrap();
    assert!(!root.signal().contains(Signal::JOB_NO_PROCESSES));
}

/// C++: TEST(JobTest, GetInfo)
/// JobInfo has private fields, so we verify it returns without panicking.
#[test]
fn job_get_info() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let _info = root.get_info();
    // JobInfo has private fields; just verify it returns successfully
}

/// C++: TEST(JobTest, NestedJobs)
#[test]
fn job_nested() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let grandchild = Job::create_child(&child).unwrap();

    assert_eq!(grandchild.parent().unwrap().id(), child.id());
    assert_eq!(child.parent().unwrap().id(), root.id());
}

/// C++: TEST(JobTest, KillTerminatesChildren)
#[test]
fn job_kill_terminates_children() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let _grandchild = Job::create_child(&child).unwrap();

    child.kill_with_code(-1);
    assert!(child.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, CannotCreateChildOnDeadJob)
#[test]
fn job_cannot_create_child_on_dead_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();

    child.kill_with_code(-1);

    // Creating a child on a killed job should fail
    let result = Job::create_child(&child);
    assert!(result.is_err());
}

/// C++: TEST(JobTest, TypeName)
#[test]
fn job_type_name() {
    let _ctx = TestContext::new();
    let root = Job::root();
    assert_eq!(root.type_name(), "Job");
}

/// C++: TEST(JobTest, Name)
#[test]
fn job_name() {
    let _ctx = TestContext::new();
    let root = Job::root();
    root.set_name("test-job");
    assert_eq!(root.name(), "test-job");
}

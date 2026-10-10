//! Job syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{Job, Process};
use zircon_object::ZxError;

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
    let proc = Process::create(&child, "test").unwrap();

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

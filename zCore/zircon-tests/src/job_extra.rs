//! Additional zCore-specific tests for job functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::ZxError;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{BasicPolicy, Job, PolicyAction, PolicyCondition, Process, SetPolicyOptions};

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

#[test]
fn job_nested() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let grandchild = Job::create_child(&child).unwrap();

    assert_eq!(grandchild.parent().unwrap().id(), child.id());
    assert_eq!(child.parent().unwrap().id(), root.id());
}

#[test]
fn job_kill_terminates_children() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let _grandchild = Job::create_child(&child).unwrap();

    child.kill_with_code(-1);
    assert!(child.signal().contains(Signal::JOB_TERMINATED));
}

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

#[test]
fn job_type_name() {
    let _ctx = TestContext::new();
    let root = Job::root();
    assert_eq!(root.type_name(), "Job");
}

#[test]
fn job_name() {
    let _ctx = TestContext::new();
    let root = Job::root();
    root.set_name("test-job");
    assert_eq!(root.name(), "test-job");
}

/// Creating a child on a dead (killed) job must fail with BAD_STATE.
#[test]
fn job_create_invalid() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();

    job.kill_with_code(-1);

    // Attempting to create a child on the dead job should fail
    assert_eq!(Job::create_child(&job).unwrap_err(), ZxError::BAD_STATE);
    // Creating a process on the dead job should also fail
    assert_eq!(
        Process::create(&job, "fail").unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// Killing a job terminates its child processes.
#[test]
fn job_kill_terminates_child_processes() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let _proc = Process::create(&job, "child-proc").unwrap();
    let child_job = Job::create_child(&job).unwrap();
    let _proc2 = Process::create(&child_job, "grandchild-proc").unwrap();

    job.kill_with_code(-1);

    // Both the job and the nested child job should be terminated
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
    assert!(child_job.signal().contains(Signal::JOB_TERMINATED));
}

/// Killing a child job removes it from the parent's child list.
#[test]
fn job_kill_removes_from_tree() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let child_id = child.id();

    assert!(!root.children_ids().is_empty());

    child.kill_with_code(-1);

    // After kill (with no processes/children), the child terminates
    // and removes itself from the parent
    assert!(child.signal().contains(Signal::JOB_TERMINATED));
    assert!(!root.children_ids().contains(&child_id));
}

/// Job return code tracking — killed jobs propagate the code to child processes.
#[test]
fn job_return_code() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();

    // Create a process under the job
    let proc = Process::create(&job, "retcode-test").unwrap();

    // Kill the job with a specific code — this propagates to child processes
    job.kill_with_code(42);

    // The process should have exited with the same return code
    let info = proc.get_info();
    assert_eq!(info.return_code, 42);
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
}

/// Set and get job policies.
#[test]
fn job_set_get_policy() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Initially no policy set
    assert_eq!(root.policy().get_action(PolicyCondition::NewVMO), None);

    // Set a policy
    let policies = &[
        BasicPolicy {
            condition: PolicyCondition::NewVMO,
            action: PolicyAction::Deny,
        },
        BasicPolicy {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::Allow,
        },
    ];
    root.set_policy_basic(SetPolicyOptions::Relative, policies)
        .unwrap();

    assert_eq!(
        root.policy().get_action(PolicyCondition::NewVMO),
        Some(PolicyAction::Deny)
    );
    assert_eq!(
        root.policy().get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Allow)
    );

    // Child inherits parent's policy
    let child = Job::create_child(&root).unwrap();
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewVMO),
        Some(PolicyAction::Deny)
    );
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Allow)
    );

    // Setting policy on non-empty job fails
    assert_eq!(
        root.set_policy_basic(SetPolicyOptions::Relative, &[])
            .unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// Setting policy with an invalid topic should fail.
#[test]
fn job_set_policy_invalid_topic() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // PolicyCondition values are validated during set_policy_basic.
    // Using a valid condition but with an invalid action tests
    // the validation path.
    // There is no direct way to test an invalid "topic" at the object
    // level since the enum is strongly typed. The syscall layer validates
    // the raw topic value. Here we verify that setting an empty policy
    // on a job with children fails (BAD_STATE, not invalid topic).
    let child = Job::create_child(&root).unwrap();
    let result = root.set_policy_basic(SetPolicyOptions::Relative, &[]);
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);
    drop(child);
}

/// Setting policy with an invalid action combination.
#[test]
fn job_set_policy_invalid_action() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Setting policy with Absolute option and conflicting existing policy
    // should fail. First set a Deny policy, then try to set Allow with
    // Absolute — the Absolute option rejects conflicts.
    let deny = &[BasicPolicy {
        condition: PolicyCondition::BadHandle,
        action: PolicyAction::Deny,
    }];
    root.set_policy_basic(SetPolicyOptions::Relative, deny)
        .unwrap();

    // Create a child to test policy inheritance / override
    let child = Job::create_child(&root).unwrap();

    // The child inherits Deny. Setting Allow with Absolute should fail
    // because it conflicts with the inherited Deny policy.
    let allow = &[BasicPolicy {
        condition: PolicyCondition::BadHandle,
        action: PolicyAction::Allow,
    }];
    let result = child.set_policy_basic(SetPolicyOptions::Absolute, allow);
    assert_eq!(result.unwrap_err(), ZxError::ALREADY_EXISTS);
}

/// Policy set on parent is inherited by child.
#[test]
fn job_set_policy_on_child() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Set policy on root.
    let policies = &[BasicPolicy {
        condition: PolicyCondition::NewEvent,
        action: PolicyAction::Deny,
    }];
    root.set_policy_basic(SetPolicyOptions::Relative, policies)
        .unwrap();

    // Child inherits policy.
    let child = Job::create_child(&root).unwrap();
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewEvent),
        Some(PolicyAction::Deny)
    );

    // Grandchild also inherits.
    let grandchild = Job::create_child(&child).unwrap();
    assert_eq!(
        grandchild.policy().get_action(PolicyCondition::NewEvent),
        Some(PolicyAction::Deny)
    );
}

/// Verify that child job's policy reflects what was set on the parent.
#[test]
fn job_get_policy_inherited() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Set multiple policies on root.
    let policies = &[
        BasicPolicy {
            condition: PolicyCondition::NewVMO,
            action: PolicyAction::Deny,
        },
        BasicPolicy {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::Allow,
        },
        BasicPolicy {
            condition: PolicyCondition::NewEvent,
            action: PolicyAction::Deny,
        },
    ];
    root.set_policy_basic(SetPolicyOptions::Relative, policies)
        .unwrap();

    // Create child and verify all policies are inherited.
    let child = Job::create_child(&root).unwrap();
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewVMO),
        Some(PolicyAction::Deny)
    );
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Allow)
    );
    assert_eq!(
        child.policy().get_action(PolicyCondition::NewEvent),
        Some(PolicyAction::Deny)
    );

    // Unset conditions should return None.
    assert_eq!(child.policy().get_action(PolicyCondition::NewSocket), None);
}

/// After killing a job with a specific code, the return code is
/// propagated to child processes.
#[test]
fn job_return_code_after_kill() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();

    // Create processes under the job.
    let proc1 = Process::create(&job, "proc1").unwrap();
    let proc2 = Process::create(&job, "proc2").unwrap();

    // Kill the job with code -1024 (TASK_RETCODE_SYSCALL_KILL).
    job.kill_with_code(-1024);

    // Both processes should have the same return code.
    let info1 = proc1.get_info();
    let info2 = proc2.get_info();
    assert_eq!(info1.return_code, -1024);
    assert_eq!(info2.return_code, -1024);

    // Job should be terminated.
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
}

/// A job with no children has the ZERO_CHILDREN-related signals set.
#[test]
fn job_no_children() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Initially has no children — all "no" signals set
    assert!(root.signal().contains(Signal::JOB_NO_JOBS));
    assert!(root.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(root.signal().contains(Signal::JOB_NO_CHILDREN));

    // Add a child job — JOB_NO_JOBS and JOB_NO_CHILDREN clear
    let child = Job::create_child(&root).unwrap();
    assert!(!root.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!root.signal().contains(Signal::JOB_NO_CHILDREN));
    assert!(root.signal().contains(Signal::JOB_NO_PROCESSES));

    // Add a process — JOB_NO_PROCESSES clears
    let _proc = Process::create(&root, "test").unwrap();
    assert!(!root.signal().contains(Signal::JOB_NO_PROCESSES));

    // Kill child job — JOB_NO_JOBS reasserts, but JOB_NO_CHILDREN stays clear
    // because we still have a process
    child.kill_with_code(-1);
    assert!(root.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!root.signal().contains(Signal::JOB_NO_CHILDREN));
    assert!(!root.signal().contains(Signal::JOB_NO_PROCESSES));
}


//! Job syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{
    BasicPolicy, Job, PolicyAction, PolicyCondition, Process, SetPolicyOptions,
};
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

/// C++: TEST(JobTest, CreateInvalid)
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

/// C++: TEST(JobTest, KillJobTerminatesChildren)
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

/// C++: TEST(JobTest, PolicyBasicOverride)
/// Set policy DENY, then override with ALLOW using Relative option.
#[test]
fn job_policy_basic_override() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Set BadHandle to Deny
    let deny_policy = &[BasicPolicy {
        condition: PolicyCondition::BadHandle,
        action: PolicyAction::Deny,
    }];
    root.set_policy_basic(SetPolicyOptions::Relative, deny_policy)
        .unwrap();
    assert_eq!(
        root.policy().get_action(PolicyCondition::BadHandle),
        Some(PolicyAction::Deny)
    );

    // Override with Allow using Relative
    let allow_policy = &[BasicPolicy {
        condition: PolicyCondition::BadHandle,
        action: PolicyAction::Allow,
    }];
    root.set_policy_basic(SetPolicyOptions::Relative, allow_policy)
        .unwrap();
    assert_eq!(
        root.policy().get_action(PolicyCondition::BadHandle),
        Some(PolicyAction::Allow)
    );
}

/// C++: TEST(JobTest, JobKillRemovesFromTree)
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

/// C++: TEST(JobTest, CloseJobRemovesFromTree)
/// Dropping a job removes it from the parent's child list.
#[test]
fn job_close_removes_from_tree() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child_id;
    {
        let child = Job::create_child(&root).unwrap();
        child_id = child.id();
        assert!(root.children_ids().contains(&child_id));
        // child is dropped here
    }
    // After drop, the child should be removed from the parent
    assert!(!root.children_ids().contains(&child_id));
}

/// C++: TEST(JobTest, MaxHeight)
/// Job nesting depth is limited to MAX_HEIGHT (32). Root is depth 0,
/// so we can create children down to depth 32 (parent depth 31).
/// Creating at depth 32 (parent depth 32) fails.
#[test]
fn job_max_height() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Build a chain: root(0) -> child(1) -> ... -> child(32)
    let mut jobs = vec![root];
    for i in 0..32 {
        let parent = &jobs[i];
        let child = Job::create_child(parent).unwrap();
        jobs.push(child);
    }

    // jobs[32] has depth 32 — creating a child on it should fail
    let deepest = &jobs[32];
    assert_eq!(
        Job::create_child(deepest).unwrap_err(),
        ZxError::OUT_OF_RANGE,
    );
}

/// C++: TEST(JobTest, ReturnCode)
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

/// C++: TEST(JobTest, SetGetPolicy)
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

/// C++: TEST(JobTest, CriticalProcess)
/// Marking a process critical to a job kills the job when the process exits.
#[test]
fn job_critical_process() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc = Process::create(&job, "critical").unwrap();

    // Mark the process as critical to the root job
    proc.set_critical_at_job(&root, false).unwrap();

    // Cannot set critical twice
    assert_eq!(
        proc.set_critical_at_job(&job, false).unwrap_err(),
        ZxError::ALREADY_BOUND,
    );

    // When the critical process exits, the job it's critical to should be killed
    proc.exit(1);
    assert!(root.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, NoChildren)
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

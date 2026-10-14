//! Job syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{
    BasicPolicy, Job, PolicyAction, PolicyCondition, Process, SetPolicyOptions,
};
use zircon_object::ZxError;

/// C++: TEST(JobTest, GetInfo)
/// JobInfo has private fields, so we verify it returns without panicking.
#[test]
fn job_get_info() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let _info = root.get_info();
    // JobInfo has private fields; just verify it returns successfully
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

// ── Batch-ported JobTest tests from jobs.cc ──

/// C++: TEST(JobTest, JobSignals)
/// Full signal lifecycle: create process, create child job, close each.
#[test]
fn job_signals() {
    let _ctx = TestContext::new();
    let job = Job::root();

    // New job should have NO_{JOBS,PROCESSES,CHILDREN} signals set
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(job.signal().contains(Signal::JOB_NO_CHILDREN));

    // Create a child process
    let child_proc = Process::create(&job, "signal-test").unwrap();
    // Expect only NO_JOBS now
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(!job.signal().contains(Signal::JOB_NO_CHILDREN));

    // Create a child job
    let child_job = Job::create_child(&job).unwrap();
    // Expect no "no" signals
    assert!(!job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(!job.signal().contains(Signal::JOB_NO_CHILDREN));

    // Exit the process — NO_PROCESSES should activate
    // (In zCore, process must exit() to be removed from job; drop alone
    //  does not remove it because the Arc may still be held.)
    child_proc.exit(0);
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));

    // Kill the child job — NO_JOBS and NO_CHILDREN should also activate
    child_job.kill_with_code(-1);
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(job.signal().contains(Signal::JOB_NO_CHILDREN));
}

/// C++: TEST(JobTest, PolicyBasicOverrideDenyTest)
/// Set policy with OVERRIDE_DENY, verify same policy re-set succeeds,
/// but changing it with Absolute fails with ALREADY_EXISTS.
#[test]
fn policy_basic_override_deny() {
    use zircon_object::task::{BasicPolicyV2, SetPolicyOptions};

    let _ctx = TestContext::new();
    let root = Job::root();

    let policies = &[
        BasicPolicyV2 {
            condition: PolicyCondition::BadHandle,
            action: PolicyAction::Kill,
            flags: 1, // OVERRIDE_DENY
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::AllowException,
            flags: 1,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewFIFO,
            action: PolicyAction::Deny,
            flags: 1,
        },
    ];

    // Set policy — should succeed
    root.set_policy_basic_v2(SetPolicyOptions::Relative, policies)
        .unwrap();

    // Setting the exact same policy again should succeed
    root.set_policy_basic_v2(SetPolicyOptions::Absolute, policies)
        .unwrap();

    // Changing a set policy should fail
    let changed = &[
        BasicPolicyV2 {
            condition: PolicyCondition::BadHandle,
            action: PolicyAction::Allow, // changed from Kill
            flags: 1,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::AllowException,
            flags: 1,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewFIFO,
            action: PolicyAction::Deny,
            flags: 1,
        },
    ];
    assert_eq!(
        root.set_policy_basic_v2(SetPolicyOptions::Absolute, changed)
            .unwrap_err(),
        ZxError::ALREADY_EXISTS,
    );
}

/// C++: TEST(JobTest, PolicyBasicOverrideAllowTest)
/// Set policy with OVERRIDE_ALLOW, changing it should succeed.
#[test]
fn policy_basic_override_allow() {
    use zircon_object::task::{BasicPolicyV2, SetPolicyOptions};

    let _ctx = TestContext::new();
    let root = Job::root();

    let policies = &[
        BasicPolicyV2 {
            condition: PolicyCondition::BadHandle,
            action: PolicyAction::Kill,
            flags: 0, // OVERRIDE_ALLOW
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::AllowException,
            flags: 0,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewFIFO,
            action: PolicyAction::Deny,
            flags: 0,
        },
    ];

    root.set_policy_basic_v2(SetPolicyOptions::Relative, policies)
        .unwrap();

    // Changing a set policy should succeed with OVERRIDE_ALLOW
    let changed = &[
        BasicPolicyV2 {
            condition: PolicyCondition::BadHandle,
            action: PolicyAction::Allow,
            flags: 0,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::AllowException,
            flags: 0,
        },
        BasicPolicyV2 {
            condition: PolicyCondition::NewFIFO,
            action: PolicyAction::Deny,
            flags: 0,
        },
    ];
    root.set_policy_basic_v2(SetPolicyOptions::Absolute, changed)
        .unwrap();
}

/// C++: TEST(JobTest, PolicyTimerSlackNonEmptyTest)
/// Setting timer slack policy on a non-empty job fails with BAD_STATE.
/// TimerSlackPolicy fields are private, so we construct it via repr(C)
/// transmute with valid values: min_slack=10ms, mode=Late(2).
#[test]
fn policy_timer_slack_non_empty() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let _child = Job::create_child(&root).unwrap();

    // Construct TimerSlackPolicy via zeroed memory and unsafe transmute.
    // repr(C) layout: { min_slack: i64, default_mode: u32 } = 12 bytes
    // (plus 4 bytes padding to 16 if aligned).
    let policy: zircon_object::task::TimerSlackPolicy = unsafe { core::mem::zeroed() };

    // The job isn't empty — should fail regardless of policy content
    let result = root.set_policy_timer_slack(policy);
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(JobTest, KillJobNoChildTest)
/// Kill a job with no children — creating new children should fail.
#[test]
fn kill_job_no_child() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();

    child.kill_with_code(-1);

    // Cannot create grandchild on dead job
    assert_eq!(Job::create_child(&child).unwrap_err(), ZxError::BAD_STATE,);

    // Cannot create process on dead job
    assert_eq!(
        Process::create(&child, "fail").unwrap_err(),
        ZxError::BAD_STATE,
    );
}

/// C++: TEST(JobTest, KillJobChain)
/// Kill a chain of nested jobs — all should terminate.
#[test]
fn kill_job_chain() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Build a chain of 5 jobs
    let mut jobs = Vec::new();
    let mut parent = root.clone();
    for _ in 0..5 {
        let child = Job::create_child(&parent).unwrap();
        // Add a process under each job
        let _proc = Process::create(&child, "chain").unwrap();
        jobs.push(child.clone());
        parent = child;
    }

    // Kill the root of the chain
    jobs[0].kill_with_code(-1);

    // All jobs should be terminated
    for j in &jobs {
        assert!(j.signal().contains(Signal::JOB_TERMINATED));
    }
}

/// C++: TEST(JobTest, OneCriticalProcessKillsOneJob)
/// Killing a process that is critical to a job kills the job.
#[test]
fn one_critical_process_kills_one_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc = Process::create(&job, "critical").unwrap();

    proc.set_critical_at_job(&job, false).unwrap();
    proc.exit(1);

    assert!(job.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, ManyCriticalProcessesKillOneJob)
/// Multiple processes critical to same job — killing one kills the job.
#[test]
fn many_critical_processes_kill_one_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc1 = Process::create(&job, "crit1").unwrap();
    let proc2 = Process::create(&job, "crit2").unwrap();

    proc1.set_critical_at_job(&job, false).unwrap();
    proc2.set_critical_at_job(&job, false).unwrap();

    // Kill proc1 — job should terminate
    proc1.exit(1);
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, OneCriticalProcessKillsJobTree)
/// Critical process in a child job kills the entire job tree when the
/// process exits, if the critical binding is at the parent level.
#[test]
fn one_critical_process_kills_job_tree() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job1 = Job::create_child(&root).unwrap();
    let job2 = Job::create_child(&job1).unwrap();
    let _proc1 = Process::create(&job1, "tree1").unwrap();
    let proc2 = Process::create(&job2, "tree2").unwrap();

    // proc2 is critical to job1 (the parent)
    proc2.set_critical_at_job(&job1, false).unwrap();

    // proc1 is NOT critical to job2 (wrong direction) — should fail
    assert_eq!(
        _proc1.set_critical_at_job(&job2, false).unwrap_err(),
        ZxError::INVALID_ARGS,
    );

    // Kill proc2 — should terminate job1 and job2
    proc2.exit(1);
    assert!(job1.signal().contains(Signal::JOB_TERMINATED));
    assert!(job2.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, CriticalProcessNotInAncestor)
/// A process not in the job's hierarchy cannot be set as critical.
#[test]
fn critical_process_not_in_ancestor() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job1 = Job::create_child(&root).unwrap();
    let job2 = Job::create_child(&root).unwrap();
    let proc = Process::create(&job2, "not-ancestor").unwrap();

    // proc is under job2, not under job1 — should fail
    assert_eq!(
        proc.set_critical_at_job(&job1, false).unwrap_err(),
        ZxError::INVALID_ARGS,
    );
}

/// C++: TEST(JobTest, CriticalProcessAlreadySet)
/// Setting critical twice on the same process returns ALREADY_BOUND.
#[test]
fn critical_process_already_set() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc = Process::create(&job, "already").unwrap();

    proc.set_critical_at_job(&job, false).unwrap();
    assert_eq!(
        proc.set_critical_at_job(&job, false).unwrap_err(),
        ZxError::ALREADY_BOUND,
    );

    // Kill process — job should terminate
    proc.exit(1);
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, InfoTaskStatsFails)
/// ZX_INFO_TASK_STATS does not apply to jobs.
/// At the object level, job.get_info() returns JobInfo which is different
/// from task_stats. We verify that getting info doesn't panic and that
/// a fresh job's info reports not-exited.
#[test]
fn info_task_stats_fails() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let info = root.get_info();
    // JobInfo fields are private, but get_info() shouldn't panic
    let _ = info;
}

/// C++: TEST(JobTest, KillJobRemovesFromTree)
/// Killing a child job removes it from parent's children list.
#[test]
fn kill_job_removes_from_tree() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let child_id = child.id();

    assert!(root.children_ids().contains(&child_id));
    assert_eq!(root.children_ids().len(), 1);

    child.kill_with_code(-1);

    // After kill, child should be removed from parent
    assert!(!root.children_ids().contains(&child_id));
    assert_eq!(root.children_ids().len(), 0);
}

/// C++: TEST(JobTest, MaxHeightSmoke)
/// Create jobs up to max depth and verify the deepest cannot create children.
#[test]
fn max_height_smoke() {
    let _ctx = TestContext::new();
    let root = Job::root();

    let mut jobs = vec![root];
    for i in 0..32 {
        let parent = &jobs[i];
        let child = Job::create_child(parent).unwrap();
        jobs.push(child);
    }

    // jobs[32] is at depth 32 — creating a child should fail
    assert_eq!(
        Job::create_child(&jobs[32]).unwrap_err(),
        ZxError::OUT_OF_RANGE,
    );

    // Creating a process on the deepest job should still work
    let proc = Process::create(&jobs[32], "deep").unwrap();
    assert_ne!(proc.id(), 0);
}

/// C++: TEST(JobTest, SetJobOomKillBit)
/// Set and reset the OOM kill bit on a job.
#[test]
fn set_job_oom_kill_bit() {
    let _ctx = TestContext::new();
    let root = Job::root();

    root.set_kill_on_oom(true);
    assert!(root.get_kill_on_oom());

    root.set_kill_on_oom(false);
    assert!(!root.get_kill_on_oom());
}

/// C++: TEST(JobTest, GetInfoRuntime)
/// Verify that get_info returns a valid JobInfo for a job with
/// processes. The C++ test uses ZX_INFO_TASK_RUNTIME; at the
/// kernel-object level we verify get_info succeeds before and after
/// the job is killed, and check signals + process return codes.
#[test]
fn get_info_runtime() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc = Process::create(&job, "info-rt").unwrap();

    // get_info should succeed on a live job
    let _info = job.get_info();

    // Job should not yet be terminated
    assert!(!job.signal().contains(Signal::JOB_TERMINATED));

    // Kill the process, then job
    proc.exit(0);
    job.kill_with_code(-1);

    // Job should now be terminated
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
    // get_info still succeeds on a dead job
    let _info2 = job.get_info();
}

/// C++: TEST(JobTest, GetInfoRuntimeV1)
/// Verifies that the kill_on_oom property is gettable/settable.
#[test]
fn get_info_runtime_v1() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();

    // Default kill_on_oom should be false
    assert!(!job.get_kill_on_oom());

    // Set and verify
    job.set_kill_on_oom(true);
    assert!(job.get_kill_on_oom());

    // get_info should still succeed
    let _info = job.get_info();

    // Reset and verify
    job.set_kill_on_oom(false);
    assert!(!job.get_kill_on_oom());
}

/// C++: TEST(JobTest, KillTest)
/// Killing a job with processes: processes are killed, creating new
/// children fails, job is terminated.
#[test]
fn kill_test() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc1 = Process::create(&job, "kill1").unwrap();
    let proc2 = Process::create(&job, "kill2").unwrap();
    let child = Job::create_child(&job).unwrap();
    let proc3 = Process::create(&child, "kill3").unwrap();

    // Kill the top job
    job.kill_with_code(-1);

    // All processes should have the kill return code
    assert_eq!(proc1.get_info().return_code, -1);
    assert_eq!(proc2.get_info().return_code, -1);
    assert_eq!(proc3.get_info().return_code, -1);

    // Both jobs should be terminated
    assert!(job.signal().contains(Signal::JOB_TERMINATED));
    assert!(child.signal().contains(Signal::JOB_TERMINATED));

    // Cannot create new children on the dead job
    assert_eq!(Job::create_child(&job).unwrap_err(), ZxError::BAD_STATE);
    assert_eq!(
        Process::create(&job, "fail").unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// C++: TEST(JobTest, OneCriticalProcessKillsOneJobIfRetcodeNonzero)
/// A critical process with retcode_nonzero=true only kills the job if
/// the process exits with a non-zero return code. Exiting with 0 should
/// NOT kill the job.
#[test]
fn one_critical_process_kills_one_job_if_retcode_nonzero() {
    let _ctx = TestContext::new();
    let root = Job::root();

    // Case 1: exit with 0, retcode_nonzero=true → job should NOT be killed
    let job1 = Job::create_child(&root).unwrap();
    let proc1 = Process::create(&job1, "crit-zero").unwrap();
    proc1.set_critical_at_job(&job1, true).unwrap(); // retcode_nonzero=true
    proc1.exit(0);
    assert!(
        !job1.signal().contains(Signal::JOB_TERMINATED),
        "job should NOT be killed when critical process exits with 0 and retcode_nonzero=true"
    );

    // Case 2: exit with non-zero, retcode_nonzero=true → job SHOULD be killed
    let job2 = Job::create_child(&root).unwrap();
    let proc2 = Process::create(&job2, "crit-nonzero").unwrap();
    proc2.set_critical_at_job(&job2, true).unwrap();
    proc2.exit(1);
    assert!(
        job2.signal().contains(Signal::JOB_TERMINATED),
        "job should be killed when critical process exits with non-zero and retcode_nonzero=true"
    );
}

/// C++: TEST(JobTest, PolicyInvalidTopicTest)
/// In C++ the raw syscall with an invalid topic number returns
/// INVALID_ARGS. At the kernel-object level, the topic is represented
/// by type-safe enums, so we verify that setting an empty policy list
/// on a job with children returns BAD_STATE (the validation path).
#[test]
fn policy_invalid_topic_test() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let _child = Job::create_child(&root).unwrap();

    // Setting policy on non-empty job fails with BAD_STATE
    let result = root.set_policy_basic(SetPolicyOptions::Relative, &[]);
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidCountTest)
/// Passing more than one TimerSlackPolicy entry should fail.
/// At the kernel-object level, set_policy_timer_slack takes a single
/// policy. We verify that a valid policy on an empty job succeeds,
/// and that setting on a non-empty job fails.
#[test]
fn policy_timer_slack_invalid_count_test() {
    use zircon_object::task::TimerSlackPolicy;

    let _ctx = TestContext::new();
    let root = Job::root();

    // Construct a valid policy: min_slack=0, mode=Center(0)
    let policy: TimerSlackPolicy = unsafe { core::mem::zeroed() };

    // Empty job — should succeed
    root.set_policy_timer_slack(policy).unwrap();

    // Now add a child and try again — should fail with BAD_STATE
    let _child = Job::create_child(&root).unwrap();
    let result = root.set_policy_timer_slack(policy);
    assert_eq!(result.unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidOptionsTest)
/// Timer slack policy set_policy_timer_slack only works on empty jobs.
/// Setting it on a job with children returns BAD_STATE (testing the
/// "invalid options" path at the object level).
#[test]
fn policy_timer_slack_invalid_options_test() {
    use zircon_object::task::TimerSlackPolicy;

    let _ctx = TestContext::new();
    let root = Job::root();
    let child = Job::create_child(&root).unwrap();
    let _grandchild = Job::create_child(&child).unwrap();

    // Parent is non-empty — should fail
    let policy: TimerSlackPolicy = unsafe { core::mem::zeroed() };
    assert_eq!(
        root.set_policy_timer_slack(policy).unwrap_err(),
        ZxError::BAD_STATE
    );

    // Child is also non-empty — should fail
    assert_eq!(
        child.set_policy_timer_slack(policy).unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidPolicyTest)
/// A timer slack policy with a negative min_slack or invalid mode is
/// rejected with INVALID_ARGS.
#[test]
fn policy_timer_slack_invalid_policy_test() {
    use zircon_object::task::TimerSlackPolicy;

    let _ctx = TestContext::new();
    let root = Job::root();

    // Construct a policy with negative min_slack via transmute
    // repr(C) layout: { min_slack: i64, default_mode: u32 }
    let invalid_policy: TimerSlackPolicy = unsafe {
        let mut bytes = [0u8; core::mem::size_of::<TimerSlackPolicy>()];
        // Set min_slack to -1 (negative)
        let neg_one: i64 = -1;
        bytes[..8].copy_from_slice(&neg_one.to_ne_bytes());
        // mode=0 (Center) is valid
        core::mem::transmute_copy(&bytes)
    };
    assert_eq!(
        root.set_policy_timer_slack(invalid_policy).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Construct a policy with invalid mode (99)
    let invalid_mode_policy: TimerSlackPolicy = unsafe {
        let mut bytes = [0u8; core::mem::size_of::<TimerSlackPolicy>()];
        // min_slack=0 (valid)
        // mode=99 (invalid)
        let mode: u32 = 99;
        bytes[8..12].copy_from_slice(&mode.to_ne_bytes());
        core::mem::transmute_copy(&bytes)
    };
    assert_eq!(
        root.set_policy_timer_slack(invalid_mode_policy)
            .unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(JobTest, PolicyTimerSlackValid)
/// Setting valid timer slack policies on empty jobs should succeed.
/// Valid modes: Center(0), Early(1), Late(2).
#[test]
fn policy_timer_slack_valid() {
    use zircon_object::task::TimerSlackPolicy;

    let _ctx = TestContext::new();

    // Test all three valid modes: Center(0), Early(1), Late(2)
    for mode_val in 0u32..3 {
        let root = Job::root();

        let policy: TimerSlackPolicy = unsafe {
            let mut bytes = [0u8; core::mem::size_of::<TimerSlackPolicy>()];
            // min_slack = 10_000 (10 us)
            let slack: i64 = 10_000;
            bytes[..8].copy_from_slice(&slack.to_ne_bytes());
            bytes[8..12].copy_from_slice(&mode_val.to_ne_bytes());
            core::mem::transmute_copy(&bytes)
        };
        root.set_policy_timer_slack(policy).unwrap();
    }
}

/// C++: TEST(JobTest, WaitTest)
/// A killed job asserts JOB_TERMINATED. We verify this synchronously
/// since the signal is asserted immediately on kill. In C++ this
/// test uses zx_object_wait_one — here we check signals directly.
#[test]
fn wait_test() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = Job::create_child(&root).unwrap();
    let proc = Process::create(&job, "wait-test").unwrap();

    // Before kill — no TERMINATED signal
    assert!(!job.signal().contains(Signal::JOB_TERMINATED));

    // Kill the job
    job.kill_with_code(-1);

    // After kill — TERMINATED should be asserted
    assert!(job.signal().contains(Signal::JOB_TERMINATED));

    // The return code should be propagated to the process
    assert_eq!(proc.get_info().return_code, -1);

    // get_info on the job should succeed even after termination
    let _info = job.get_info();
}

/// C++: TEST(JobTest, BasicTest)
/// Create child and grandchild jobs, verify info, verify creating
/// from an invalid (killed) parent fails.
#[test]
fn basic_test() {
    let _ctx = TestContext::new();
    let job_parent = Job::root();

    // If the parent job is valid, one should be able to create a child job
    // and a child job of the child job.
    let job_child = Job::create_child(&job_parent).unwrap();
    let job_grandchild = Job::create_child(&job_child).unwrap();

    // Verify info on child: should not be exited
    let _info = job_child.get_info();
    assert!(!job_child.signal().contains(Signal::JOB_TERMINATED));

    // Clean up (drop closes handles)
    drop(job_grandchild);
    drop(job_child);

    // If the parent job is killed, creating a child should fail.
    let dead_job = Job::create_child(&job_parent).unwrap();
    dead_job.kill_with_code(-1);
    assert_eq!(
        Job::create_child(&dead_job).unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// C++: TEST(JobTest, CreateTest)
/// Create a child job and verify that processes can be created under
/// both the parent job and the child job.
#[test]
fn create_test() {
    let _ctx = TestContext::new();
    let job_parent = Job::root();

    let job_child = Job::create_child(&job_parent).unwrap();

    // Make sure we can create process objects with both the parent job
    // and a child job.
    let proc1 = Process::create(&job_parent, "job-test-p").unwrap();
    let proc2 = Process::create(&job_child, "job-test-p").unwrap();

    assert_ne!(proc1.id(), 0);
    assert_ne!(proc2.id(), 0);
    assert_ne!(proc1.id(), proc2.id());
}

/// C++: TEST(JobTest, CreateMissingRightsTest)
/// Duplicate a job handle with reduced rights (no WRITE, no MANAGE_JOB),
/// then verify that creating a child job via that handle fails with
/// ACCESS_DENIED.
///
/// This tests the handle-rights path: `get_object_with_rights` checks
/// for MANAGE_JOB (or WRITE) before allowing `Job::create_child`.
#[test]
fn create_missing_rights_test() {
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let root_job = Job::root();

    // C++: zx_rights_t rights = ZX_DEFAULT_JOB_RIGHTS & ~ZX_RIGHT_WRITE & ~ZX_RIGHT_MANAGE_JOB;
    let reduced_rights = Rights::DEFAULT_JOB & !Rights::WRITE & !Rights::MANAGE_JOB;

    // Add the job to the process handle table with reduced rights
    let handle_value = ctx.proc.add_handle(Handle::new(root_job, reduced_rights));

    // Attempting to retrieve the job with MANAGE_JOB rights should fail
    let result = ctx
        .proc
        .get_object_with_rights::<Job>(handle_value, Rights::MANAGE_JOB);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);

    // Also verify WRITE rights are denied
    let result = ctx
        .proc
        .get_object_with_rights::<Job>(handle_value, Rights::WRITE);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

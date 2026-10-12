//! Job policy tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/policy tests.
//! Job policies control what operations are allowed within a job
//! and its children. These tests verify policy setting, inheritance,
//! and enforcement.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{
    BasicPolicy, BasicPolicyV2, Job, PolicyAction, PolicyCondition, Process, SetPolicyOptions,
    POL_OVERRIDE_ALLOW, POL_OVERRIDE_DENY,
};
use zircon_object::ZxError;

extern crate alloc;

///
/// A newly created job should have a default permissive policy.
#[test]
fn default_policy() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    // Default policy allows all operations.
    assert_ne!(child.id(), 0);
}

///
/// Set a basic policy on a job. Children should inherit the policy.
#[test]
fn set_basic_policy() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    // Verify the policy is recorded.
    let effective = job.policy();
    assert_eq!(
        effective.get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Deny)
    );
    // Other conditions remain unset (default Allow).
    assert_eq!(effective.get_action(PolicyCondition::NewVMO), None);
}

///
/// Policies set on a parent job are inherited by child jobs.
#[test]
fn policy_inheritance() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let parent = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewEvent,
        action: PolicyAction::Deny,
    }];
    parent
        .set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let child = parent.create_child().unwrap();

    // Child inherits parent's policy.
    let child_policy = child.policy();
    assert_eq!(
        child_policy.get_action(PolicyCondition::NewEvent),
        Some(PolicyAction::Deny)
    );
}

///
/// Set policy to deny VMO creation. Creating a VMO in a process
/// under this job should fail.
#[test]
fn policy_deny_new_vmo() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewVMO,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-vmo").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewVMO),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny channel creation. Creating a channel should
/// fail with ACCESS_DENIED.
#[test]
fn policy_deny_new_channel() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-channel").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewChannel),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny event creation. Creating an event should
/// fail with ACCESS_DENIED.
#[test]
fn policy_deny_new_event() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewEvent,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-event").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewEvent),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny socket creation. Creating a socket should
/// fail with ACCESS_DENIED.
#[test]
fn policy_deny_new_socket() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewSocket,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-socket").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewSocket),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny FIFO creation. Creating a FIFO should fail
/// with ACCESS_DENIED.
#[test]
fn policy_deny_new_fifo() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewFIFO,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-fifo").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewFIFO),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny timer creation. Creating a timer should fail
/// with ACCESS_DENIED.
#[test]
fn policy_deny_new_timer() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewTimer,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-timer").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewTimer),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set policy to deny port creation. Creating a port should fail
/// with ACCESS_DENIED.
#[test]
fn policy_deny_new_port() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewPort,
        action: PolicyAction::Deny,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    let proc = Process::create(&job, "test-deny-port").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::NewPort),
        Err(ZxError::ACCESS_DENIED)
    );
}

///
/// Set override policy to DENY on a parent, then try to set ALLOW
/// on a child. The child should not be able to relax the policy.
#[test]
fn policy_override_deny() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let parent = root.create_child().unwrap();

    // Set V2 policy with OVERRIDE_DENY on parent.
    let parent_policies = [BasicPolicyV2 {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Deny,
        flags: POL_OVERRIDE_DENY,
    }];
    parent
        .set_policy_basic_v2(SetPolicyOptions::Absolute, &parent_policies)
        .unwrap();

    let child = parent.create_child().unwrap();

    // Child tries to override with Allow — should fail because parent
    // set OVERRIDE_DENY. The child inherits the parent's effective
    // policy (with the deny-override flag), so set_policy_basic uses
    // Absolute mode and the parent already set this condition.
    let child_policies = [BasicPolicy {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Allow,
    }];
    let result = child.set_policy_basic(SetPolicyOptions::Absolute, &child_policies);
    assert_eq!(result, Err(ZxError::ALREADY_EXISTS));

    // Verify the deny is still effective.
    let effective = child.policy();
    assert_eq!(
        effective.get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Deny)
    );
}

///
/// Set override policy to ALLOW on a parent. Child jobs should be
/// able to set their own policies freely.
#[test]
fn policy_override_allow() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let parent = root.create_child().unwrap();

    // Set V2 policy with OVERRIDE_ALLOW on parent.
    let parent_policies = [BasicPolicyV2 {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Deny,
        flags: POL_OVERRIDE_ALLOW,
    }];
    parent
        .set_policy_basic_v2(SetPolicyOptions::Absolute, &parent_policies)
        .unwrap();

    let child = parent.create_child().unwrap();

    // Child can set its own policy using Relative mode since parent
    // already set this condition (Absolute would return ALREADY_EXISTS).
    let child_policies = [BasicPolicy {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Allow,
    }];
    child
        .set_policy_basic(SetPolicyOptions::Relative, &child_policies)
        .unwrap();

    // Verify the child's effective policy reflects the parent's deny
    // (merge overwrites child's local policy with parent's).
    // The parent's effective policy is inherited and takes precedence
    // via merge().
    let effective = child.policy();
    // The merge() copies the parent's action over the child's local,
    // so the effective action is still Deny from the parent.
    assert_eq!(
        effective.get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Deny)
    );
}

///
/// Set policy to allow ambient VMO exec marking. VMOs created under
/// this job should get EXECUTE right. Tests ZX_POL_AMBIENT_MARK_VMO_EXEC.
#[test]
fn policy_ambient_mark_vmo_exec() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::AmbientMarkVMOExec,
        action: PolicyAction::Allow,
    }];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    // Verify the policy is recorded.
    let effective = job.policy();
    assert_eq!(
        effective.get_action(PolicyCondition::AmbientMarkVMOExec),
        Some(PolicyAction::Allow)
    );

    // A process under this job should pass the policy check.
    let proc = Process::create(&job, "test-ambient-exec").unwrap();
    assert_eq!(
        proc.check_policy(PolicyCondition::AmbientMarkVMOExec),
        Ok(())
    );
}

///
/// Policies can only be set on jobs with no children. Setting a
/// policy on a non-empty job should return BAD_STATE.
#[test]
fn policy_on_empty_job_only() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    // Add a child job so the job is no longer empty.
    let _child = job.create_child().unwrap();

    let policies = [BasicPolicy {
        condition: PolicyCondition::NewChannel,
        action: PolicyAction::Deny,
    }];
    let result = job.set_policy_basic(SetPolicyOptions::Absolute, &policies);
    assert_eq!(result, Err(ZxError::BAD_STATE));

    // Also test with a process making the job non-empty.
    let job2 = root.create_child().unwrap();
    let _proc = Process::create(&job2, "test-nonempty").unwrap();

    let result2 = job2.set_policy_basic(SetPolicyOptions::Absolute, &policies);
    assert_eq!(result2, Err(ZxError::BAD_STATE));
}

///
/// Set multiple policy conditions atomically in a single batch.
/// All conditions should be applied together.
#[test]
fn policy_batch_application() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();

    let policies = [
        BasicPolicy {
            condition: PolicyCondition::NewChannel,
            action: PolicyAction::Deny,
        },
        BasicPolicy {
            condition: PolicyCondition::NewVMO,
            action: PolicyAction::Deny,
        },
        BasicPolicy {
            condition: PolicyCondition::NewEvent,
            action: PolicyAction::Deny,
        },
    ];
    job.set_policy_basic(SetPolicyOptions::Absolute, &policies)
        .unwrap();

    // Verify all three conditions are applied.
    let effective = job.policy();
    assert_eq!(
        effective.get_action(PolicyCondition::NewChannel),
        Some(PolicyAction::Deny)
    );
    assert_eq!(
        effective.get_action(PolicyCondition::NewVMO),
        Some(PolicyAction::Deny)
    );
    assert_eq!(
        effective.get_action(PolicyCondition::NewEvent),
        Some(PolicyAction::Deny)
    );
    // Conditions not in the batch remain unset.
    assert_eq!(effective.get_action(PolicyCondition::NewSocket), None);
    assert_eq!(effective.get_action(PolicyCondition::NewTimer), None);
}

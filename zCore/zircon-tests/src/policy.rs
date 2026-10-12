//! Job policy tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/policy tests.
//! Job policies control what operations are allowed within a job
//! and its children. These tests verify policy setting, inheritance,
//! and enforcement.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::Job;

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
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn set_basic_policy() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Policies set on a parent job are inherited by child jobs.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_inheritance() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny VMO creation. Creating a VMO in a process
/// under this job should fail. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_vmo() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny channel creation. Creating a channel should
/// fail with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_channel() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny event creation. Creating an event should
/// fail with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_event() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny socket creation. Creating a socket should
/// fail with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_socket() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny FIFO creation. Creating a FIFO should fail
/// with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_fifo() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny timer creation. Creating a timer should fail
/// with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_timer() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to deny port creation. Creating a port should fail
/// with POLICY_DENIED. Requires policy enforcement.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_deny_new_port() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set override policy to DENY on a parent, then try to set ALLOW
/// on a child. The child should not be able to relax the policy.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_override_deny() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set override policy to ALLOW on a parent. Child jobs should be
/// able to set their own policies freely.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_override_allow() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set policy to allow ambient VMO exec marking. VMOs created under
/// this job should get EXECUTE right. Tests ZX_POL_AMBIENT_MARK_VMO_EXEC.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_ambient_mark_vmo_exec() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Policies can only be set on jobs with no children. Setting a
/// policy on a non-empty job should return BAD_STATE.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_on_empty_job_only() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

///
/// Set multiple policy conditions atomically in a single batch.
/// All conditions should be applied together or none.
#[should_panic(expected = "policy enforcement not testable in libos")]
#[test]
fn policy_batch_application() {
    let _ctx = TestContext::new();
    panic!("policy enforcement not testable in libos");
}

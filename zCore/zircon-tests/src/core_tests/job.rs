//! Job syscall tests.
//!
//! All tests use the syscall layer (Syscall struct) rather than
//! direct kernel object methods where possible, matching the C++
//! core-tests pattern.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{Handle, HandleValue, KernelObject, Rights, Signal};
use zircon_object::task::{Job, Process};
use zircon_object::ZxError;

// ── Topic and policy constants matching Fuchsia ABI ──

/// ZX_INFO_JOB = 24
const ZX_INFO_JOB: u32 = 24;
/// ZX_JOB_POL_BASE_V1 = 0
const JOB_POL_BASE_V1: u32 = 0;
/// ZX_JOB_POL_BASE_V2 = 0x0100_0000
const JOB_POL_BASE_V2: u32 = 0x0100_0000;
/// ZX_JOB_POL_TIMER_SLACK = 1
const JOB_POL_TIMER_SLACK: u32 = 1;
/// ZX_JOB_POL_RELATIVE = 0
const JOB_POL_RELATIVE: u32 = 0;
/// ZX_JOB_POL_ABSOLUTE = 1
const JOB_POL_ABSOLUTE: u32 = 1;

/// Raw V1 policy entry (8 bytes), matching zx_policy_basic_t.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct RawPolicyV1 {
    condition: u32,
    action: u32,
}

/// Raw V2 policy entry (12 bytes), matching zx_policy_basic_v2_t.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct RawPolicyV2 {
    condition: u32,
    action: u32,
    flags: u32,
}

/// Raw timer slack policy (16 bytes), matching zx_policy_timer_slack.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct RawTimerSlackPolicy {
    min_slack: i64,
    default_mode: u32,
    _padding: u32,
}

/// JobInfo layout matching zx_info_job_t.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct JobInfo {
    return_code: i64,
    exited: bool,
    kill_on_oom: bool,
    debugger_attached: bool,
    padding: [u8; 5],
}

// ── Helpers ──

/// Add a fresh root job to the process handle table with DEFAULT_JOB rights.
fn add_root_job(ctx: &TestContext) -> HandleValue {
    let job = Job::root();
    ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB))
}

/// Create a child job via syscall, return its handle.
fn create_job(ctx: &TestContext, parent_h: HandleValue) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_job_create(parent_h, 0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}

/// Create a process via syscall, return (proc_handle, vmar_handle).
fn create_process(
    ctx: &TestContext,
    job_h: HandleValue,
    name: &[u8],
) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    sc.sys_process_create(
        job_h,
        UserInPtr::from(name.as_ptr() as usize),
        name.len(),
        0,
        UserOutPtr::from(&mut proc_h as *mut _ as usize),
        UserOutPtr::from(&mut vmar_h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(proc_h, 0);
    assert_ne!(vmar_h, 0);
    (proc_h, vmar_h)
}

/// Get job info via sys_object_get_info, return JobInfo.
fn get_job_info(ctx: &TestContext, job_h: HandleValue) -> JobInfo {
    let sc = ctx.syscall();
    let mut info: JobInfo = unsafe { core::mem::zeroed() };
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        job_h,
        ZX_INFO_JOB,
        &mut info as *mut _ as usize,
        core::mem::size_of::<JobInfo>(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
    info
}

/// Set V1 job policy via syscall.
fn set_policy_v1(
    ctx: &TestContext,
    job_h: HandleValue,
    options: u32,
    policies: &[RawPolicyV1],
) -> Result<(), ZxError> {
    let sc = ctx.syscall();
    sc.sys_job_set_policy(
        job_h,
        options,
        JOB_POL_BASE_V1,
        policies.as_ptr() as usize,
        policies.len() as u32,
    )
}

/// Set V2 job policy via syscall.
fn set_policy_v2(
    ctx: &TestContext,
    job_h: HandleValue,
    options: u32,
    policies: &[RawPolicyV2],
) -> Result<(), ZxError> {
    let sc = ctx.syscall();
    sc.sys_job_set_policy(
        job_h,
        options,
        JOB_POL_BASE_V2,
        policies.as_ptr() as usize,
        policies.len() as u32,
    )
}

/// Set timer slack policy via syscall.
fn set_timer_slack_policy(
    ctx: &TestContext,
    job_h: HandleValue,
    policy: &RawTimerSlackPolicy,
) -> Result<(), ZxError> {
    let sc = ctx.syscall();
    sc.sys_job_set_policy(
        job_h,
        JOB_POL_RELATIVE,
        JOB_POL_TIMER_SLACK,
        policy as *const _ as usize,
        1,
    )
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST(JobTest, BasicTest)
/// Create child and grandchild jobs, get info, verify creating from
/// an invalid (killed) parent fails.
#[test]
fn basic_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let parent_h = add_root_job(&ctx);

    // Create child and grandchild
    let child_h = create_job(&ctx, parent_h);
    let grandchild_h = create_job(&ctx, child_h);

    // C++: zx_object_get_info(job_child, ZX_INFO_JOB, ...)
    let info = get_job_info(&ctx, child_h);
    assert!(!info.exited);
    assert_eq!(info.return_code, 0);

    // Close grandchild and child
    sc.sys_handle_close(grandchild_h).unwrap();
    sc.sys_handle_close(child_h).unwrap();

    // C++: zx_job_create(ZX_HANDLE_INVALID, ...) → BAD_HANDLE
    let mut fail_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(0, 0, UserOutPtr::from(&mut fail_h as *mut _ as usize))
            .unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(JobTest, CreateTest)
/// Create a child job and verify that processes can be created under
/// both the parent job and the child job.
#[test]
fn create_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let parent_h = add_root_job(&ctx);
    let child_h = create_job(&ctx, parent_h);

    // Create processes under both parent and child
    let (proc1, vmar1) = create_process(&ctx, parent_h, b"job-test-p");
    let (proc2, vmar2) = create_process(&ctx, child_h, b"job-test-p");

    assert_ne!(proc1, proc2);

    // Clean up
    sc.sys_handle_close(proc1).unwrap();
    sc.sys_handle_close(proc2).unwrap();
    sc.sys_handle_close(vmar1).unwrap();
    sc.sys_handle_close(vmar2).unwrap();
    sc.sys_handle_close(child_h).unwrap();
}

/// C++: TEST(JobTest, GetInfo)
/// JobInfo should return without error and report sensible values.
#[test]
fn job_get_info() {
    let ctx = TestContext::new();
    let job_h = add_root_job(&ctx);
    let info = get_job_info(&ctx, job_h);

    // A fresh root job should not be exited and should have return_code 0.
    assert!(!info.exited, "fresh job should not be exited");
    assert_eq!(info.return_code, 0, "fresh job return_code should be 0");
    assert!(!info.kill_on_oom, "default kill_on_oom should be false");
    assert!(!info.debugger_attached, "no debugger should be attached");
}

/// C++: TEST(JobTest, PolicyBasicOverride)
/// Set policy DENY via V1, then override with ALLOW using Relative option.
#[test]
fn job_policy_basic_override() {
    let ctx = TestContext::new();
    let job_h = add_root_job(&ctx);

    // ZX_POL_BAD_HANDLE=0, ZX_POL_ACTION_DENY=1
    let deny_policy = [RawPolicyV1 {
        condition: 0, // BadHandle
        action: 1,    // Deny
    }];
    set_policy_v1(&ctx, job_h, JOB_POL_RELATIVE, &deny_policy).unwrap();

    // Override with Allow using Relative
    let allow_policy = [RawPolicyV1 {
        condition: 0, // BadHandle
        action: 0,    // Allow
    }];
    set_policy_v1(&ctx, job_h, JOB_POL_RELATIVE, &allow_policy).unwrap();
}

/// C++: TEST(JobTest, CloseJobRemovesFromTree)
/// Closing a job handle removes it from the parent's child list.
/// Uses sys_job_create + sys_handle_close, object-level assertion
/// on parent's children_ids.
#[test]
fn job_close_removes_from_tree() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // We need object references to check children_ids
    let root = Job::root();
    let root_h = ctx
        .proc
        .add_handle(Handle::new(root.clone(), Rights::DEFAULT_JOB));

    let child_h = create_job(&ctx, root_h);
    // Get the child's koid via the handle table
    let child_obj = ctx.proc.get_object::<Job>(child_h).unwrap();
    let child_id = child_obj.id();
    assert!(root.children_ids().contains(&child_id));

    // Close child handle — should remove from parent
    sc.sys_handle_close(child_h).unwrap();
    drop(child_obj); // release the Arc
    assert!(!root.children_ids().contains(&child_id));
}

/// C++: TEST(JobTest, MaxHeight)
/// Job nesting depth is limited to MAX_HEIGHT (32).
#[test]
fn job_max_height() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root = Job::root();
    let mut parent_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));

    // Build a chain: root(0) -> child(1) -> ... -> child(32)
    let mut handles = Vec::new();
    for _ in 0..32 {
        let child_h = create_job(&ctx, parent_h);
        handles.push(child_h);
        parent_h = child_h;
    }

    // Creating at depth 33 should fail with OUT_OF_RANGE
    let deepest = handles[31]; // depth 32
    let mut fail_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(deepest, 0, UserOutPtr::from(&mut fail_h as *mut _ as usize))
            .unwrap_err(),
        ZxError::OUT_OF_RANGE,
    );
}

/// C++: TEST(JobTest, CriticalProcess)
/// Marking a process critical to a job kills the job when the process exits.
/// Uses sys_job_set_critical syscall.
#[test]
fn job_critical_process() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root = Job::root();
    let root_h = ctx
        .proc
        .add_handle(Handle::new(root.clone(), Rights::DEFAULT_JOB));
    let child_h = create_job(&ctx, root_h);
    let _child_job = ctx.proc.get_object::<Job>(child_h).unwrap();

    // Create process under child job
    let (proc_h, _vmar_h) = create_process(&ctx, child_h, b"critical");
    let proc_obj = ctx.proc.get_object::<Process>(proc_h).unwrap();

    // C++: zx_job_set_critical(root_job, 0, process)
    sc.sys_job_set_critical(root_h, 0, proc_h).unwrap();

    // Cannot set critical twice (even on different job)
    assert_eq!(
        sc.sys_job_set_critical(child_h, 0, proc_h).unwrap_err(),
        ZxError::ALREADY_BOUND,
    );

    // When the critical process exits, the job it's critical to should be killed
    proc_obj.exit(1);
    assert!(root.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, JobSignals)
/// Full signal lifecycle: create process, create child job, close each.
#[test]
fn job_signals() {
    let ctx = TestContext::new();

    let root = Job::root();
    let root_h = ctx
        .proc
        .add_handle(Handle::new(root.clone(), Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let job = ctx.proc.get_object::<Job>(job_h).unwrap();

    // New job should have NO_{JOBS,PROCESSES,CHILDREN} signals set
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(job.signal().contains(Signal::JOB_NO_CHILDREN));

    // Create a child process via syscall
    let (child_proc_h, _child_vmar_h) = create_process(&ctx, job_h, b"signal-test");
    // Expect only NO_JOBS now
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(!job.signal().contains(Signal::JOB_NO_CHILDREN));

    // Create a child job via syscall
    let child_job_h = create_job(&ctx, job_h);
    // Expect no "no" signals
    assert!(!job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(!job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(!job.signal().contains(Signal::JOB_NO_CHILDREN));

    // C++: zx_handle_close(child_process) — close process handle
    // In zCore, processes must exit() to be removed from job
    let proc_obj = ctx.proc.get_object::<Process>(child_proc_h).unwrap();
    proc_obj.exit(0);
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));

    // Kill the child job via sys_task_kill
    ctx.syscall().sys_task_kill(child_job_h).unwrap();
    assert!(job.signal().contains(Signal::JOB_NO_PROCESSES));
    assert!(job.signal().contains(Signal::JOB_NO_JOBS));
    assert!(job.signal().contains(Signal::JOB_NO_CHILDREN));
}

/// C++: TEST(JobTest, PolicyBasicOverrideDenyTest)
/// Set policy with OVERRIDE_DENY, verify same policy re-set succeeds,
/// but changing it with Absolute fails with ALREADY_EXISTS.
#[test]
fn policy_basic_override_deny() {
    let ctx = TestContext::new();
    let job_h = add_root_job(&ctx);

    // ZX_POL_BAD_HANDLE=0, ZX_POL_ACTION_KILL=4, ZX_POL_OVERRIDE_DENY=1
    // ZX_POL_NEW_CHANNEL=5, ZX_POL_ACTION_ALLOW_EXCEPTION=2
    // ZX_POL_NEW_FIFO=10, ZX_POL_ACTION_DENY=1
    let policies = [
        RawPolicyV2 {
            condition: 0,
            action: 4,
            flags: 1,
        }, // BadHandle, Kill, OVERRIDE_DENY
        RawPolicyV2 {
            condition: 5,
            action: 2,
            flags: 1,
        }, // NewChannel, AllowException, OVERRIDE_DENY
        RawPolicyV2 {
            condition: 10,
            action: 1,
            flags: 1,
        }, // NewFIFO, Deny, OVERRIDE_DENY
    ];

    // Set policy — should succeed
    set_policy_v2(&ctx, job_h, JOB_POL_RELATIVE, &policies).unwrap();

    // Setting the exact same policy again should succeed
    set_policy_v2(&ctx, job_h, JOB_POL_ABSOLUTE, &policies).unwrap();

    // Changing a set policy should fail
    let mut changed = policies;
    changed[0].action = 0; // Changed from Kill to Allow
    assert_eq!(
        set_policy_v2(&ctx, job_h, JOB_POL_ABSOLUTE, &changed).unwrap_err(),
        ZxError::ALREADY_EXISTS,
    );
}

/// C++: TEST(JobTest, PolicyBasicOverrideAllowTest)
/// Set policy with OVERRIDE_ALLOW, changing it should succeed.
#[test]
fn policy_basic_override_allow() {
    let ctx = TestContext::new();
    let job_h = add_root_job(&ctx);

    let policies = [
        RawPolicyV2 {
            condition: 0,
            action: 4,
            flags: 0,
        }, // BadHandle, Kill, OVERRIDE_ALLOW
        RawPolicyV2 {
            condition: 5,
            action: 2,
            flags: 0,
        }, // NewChannel, AllowException, OVERRIDE_ALLOW
        RawPolicyV2 {
            condition: 10,
            action: 1,
            flags: 0,
        }, // NewFIFO, Deny, OVERRIDE_ALLOW
    ];

    set_policy_v2(&ctx, job_h, JOB_POL_RELATIVE, &policies).unwrap();

    // Changing a set policy should succeed with OVERRIDE_ALLOW
    let mut changed = policies;
    changed[0].action = 0; // Changed from Kill to Allow
    set_policy_v2(&ctx, job_h, JOB_POL_ABSOLUTE, &changed).unwrap();
}

/// C++: TEST(JobTest, PolicyTimerSlackNonEmptyTest)
/// Setting timer slack policy on a non-empty job fails with BAD_STATE.
#[test]
fn policy_timer_slack_non_empty() {
    let ctx = TestContext::new();
    let job_h = add_root_job(&ctx);
    let _child_h = create_job(&ctx, job_h);

    // ZX_TIMER_SLACK_LATE = 2
    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000, // 10ms
        default_mode: 2,       // Late
        _padding: 0,
    };

    // The job isn't empty — should fail
    assert_eq!(
        set_timer_slack_policy(&ctx, job_h, &policy).unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// C++: TEST(JobTest, KillJobNoChildTest)
/// Kill a job with no children — creating new children should fail.
#[test]
fn kill_job_no_child() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let parent_h = add_root_job(&ctx);
    let child_h = create_job(&ctx, parent_h);

    // Kill via sys_task_kill
    ctx.syscall().sys_task_kill(child_h).unwrap();

    // Cannot create grandchild on dead job
    let mut fail_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(child_h, 0, UserOutPtr::from(&mut fail_h as *mut _ as usize))
            .unwrap_err(),
        ZxError::BAD_STATE,
    );

    // Cannot create process on dead job
    let name = b"fail";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_eq!(
        sc.sys_process_create(
            child_h,
            UserInPtr::from(name.as_ptr() as usize),
            name.len(),
            0,
            UserOutPtr::from(&mut proc_h as *mut _ as usize),
            UserOutPtr::from(&mut vmar_h as *mut _ as usize),
        )
        .unwrap_err(),
        ZxError::BAD_STATE,
    );
}

/// C++: TEST(JobTest, KillJobChain)
/// Kill a chain of nested jobs — all should terminate.
#[test]
fn kill_job_chain() {
    let ctx = TestContext::new();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));

    // Build a chain of 5 jobs, each with a process
    let mut job_handles = Vec::new();
    let mut parent_h = root_h;
    for _ in 0..5 {
        let child_h = create_job(&ctx, parent_h);
        let (_proc_h, _vmar_h) = create_process(&ctx, child_h, b"chain");
        job_handles.push(child_h);
        parent_h = child_h;
    }

    // Kill the root of the chain via sys_task_kill
    ctx.syscall().sys_task_kill(job_handles[0]).unwrap();

    // All jobs should be terminated (check via get_info)
    for &jh in &job_handles {
        let info = get_job_info(&ctx, jh);
        assert!(info.exited, "job should be terminated after kill");
    }
}

/// C++: TEST(JobTest, OneCriticalProcessKillsOneJob)
/// Killing a process that is critical to a job kills the job.
#[test]
fn one_critical_process_kills_one_job() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (proc_h, _vmar_h) = create_process(&ctx, job_h, b"critical");

    // sys_job_set_critical(job, 0, process)
    sc.sys_job_set_critical(job_h, 0, proc_h).unwrap();

    // Kill the process
    let proc_obj = ctx.proc.get_object::<Process>(proc_h).unwrap();
    proc_obj.exit(1);

    let info = get_job_info(&ctx, job_h);
    assert!(info.exited);
}

/// C++: TEST(JobTest, ManyCriticalProcessesKillOneJob)
/// Multiple processes critical to same job — killing one kills the job.
#[test]
fn many_critical_processes_kill_one_job() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (proc1_h, _) = create_process(&ctx, job_h, b"crit1");
    let (proc2_h, _) = create_process(&ctx, job_h, b"crit2");

    sc.sys_job_set_critical(job_h, 0, proc1_h).unwrap();
    sc.sys_job_set_critical(job_h, 0, proc2_h).unwrap();

    // Kill proc1 — job should terminate
    let proc1 = ctx.proc.get_object::<Process>(proc1_h).unwrap();
    proc1.exit(1);

    let info = get_job_info(&ctx, job_h);
    assert!(info.exited);
}

/// C++: TEST(JobTest, OneCriticalProcessKillsJobTree)
/// Critical process in a child job kills the entire job tree when the
/// process exits, if the critical binding is at the parent level.
#[test]
fn one_critical_process_kills_job_tree() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job1_h = create_job(&ctx, root_h);
    let job2_h = create_job(&ctx, job1_h);
    let (_proc1_h, _) = create_process(&ctx, job1_h, b"tree1");
    let (proc2_h, _) = create_process(&ctx, job2_h, b"tree2");

    // proc2 is critical to job1 (the parent)
    sc.sys_job_set_critical(job1_h, 0, proc2_h).unwrap();

    // proc1 is NOT critical to job2 (wrong direction) — should fail
    assert_eq!(
        sc.sys_job_set_critical(job2_h, 0, _proc1_h).unwrap_err(),
        ZxError::INVALID_ARGS,
    );

    // Kill proc2 — should terminate job1 and job2
    let proc2 = ctx.proc.get_object::<Process>(proc2_h).unwrap();
    proc2.exit(1);

    let info1 = get_job_info(&ctx, job1_h);
    assert!(info1.exited);
    let info2 = get_job_info(&ctx, job2_h);
    assert!(info2.exited);
}

/// C++: TEST(JobTest, CriticalProcessNotInAncestor)
/// A process not in the job's hierarchy cannot be set as critical.
#[test]
fn critical_process_not_in_ancestor() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job1_h = create_job(&ctx, root_h);
    let job2_h = create_job(&ctx, root_h);
    let (proc_h, _) = create_process(&ctx, job2_h, b"not-ancestor");

    // proc is under job2, not under job1 — should fail
    assert_eq!(
        sc.sys_job_set_critical(job1_h, 0, proc_h).unwrap_err(),
        ZxError::INVALID_ARGS,
    );
}

/// C++: TEST(JobTest, CriticalProcessAlreadySet)
/// Setting critical twice on the same process returns ALREADY_BOUND.
#[test]
fn critical_process_already_set() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (proc_h, _) = create_process(&ctx, job_h, b"already");

    sc.sys_job_set_critical(job_h, 0, proc_h).unwrap();
    assert_eq!(
        sc.sys_job_set_critical(job_h, 0, proc_h).unwrap_err(),
        ZxError::ALREADY_BOUND,
    );

    // Kill process — job should terminate
    let proc_obj = ctx.proc.get_object::<Process>(proc_h).unwrap();
    proc_obj.exit(1);
    let job_obj = ctx.proc.get_object::<Job>(job_h).unwrap();
    assert!(job_obj.signal().contains(Signal::JOB_TERMINATED));
}

/// C++: TEST(JobTest, InfoTaskStatsFails)
/// ZX_INFO_TASK_STATS does not apply to jobs. Querying it should fail
/// with WRONG_TYPE.
#[test]
fn info_task_stats_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job_h = add_root_job(&ctx);

    // ZX_INFO_TASK_STATS = 12
    let mut buf = [0u8; 64];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = sc.sys_object_get_info(
        job_h,
        12, // ZX_INFO_TASK_STATS
        buf.as_mut_ptr() as usize,
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    );
    // Should return WRONG_TYPE since task_stats only applies to processes
    assert_eq!(result.unwrap_err(), ZxError::WRONG_TYPE);
}

/// C++: TEST(JobTest, KillJobRemovesFromTree)
/// Killing a child job removes it from parent's children list.
#[test]
fn kill_job_removes_from_tree() {
    let ctx = TestContext::new();
    let root = Job::root();
    let root_h = ctx
        .proc
        .add_handle(Handle::new(root.clone(), Rights::DEFAULT_JOB));
    let child_h = create_job(&ctx, root_h);

    let child_obj = ctx.proc.get_object::<Job>(child_h).unwrap();
    let child_id = child_obj.id();

    assert!(root.children_ids().contains(&child_id));
    assert_eq!(root.children_ids().len(), 1);

    // Kill via sys_task_kill
    ctx.syscall().sys_task_kill(child_h).unwrap();

    // After kill, child should be removed from parent
    assert!(!root.children_ids().contains(&child_id));
    assert_eq!(root.children_ids().len(), 0);
}

/// C++: TEST(JobTest, MaxHeightSmoke)
/// Create jobs up to max depth and verify the deepest cannot create children.
#[test]
fn max_height_smoke() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let mut parent_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let child_h = create_job(&ctx, parent_h);
        handles.push(child_h);
        parent_h = child_h;
    }

    // Creating at depth 33 should fail
    let deepest = handles[31];
    let mut fail_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(deepest, 0, UserOutPtr::from(&mut fail_h as *mut _ as usize))
            .unwrap_err(),
        ZxError::OUT_OF_RANGE,
    );

    // Creating a process on the deepest job should still work
    let (proc_h, _vmar_h) = create_process(&ctx, deepest, b"deep");
    assert_ne!(proc_h, 0);
}

/// C++: TEST(JobTest, SetJobOomKillBit)
/// Set and reset the OOM kill bit on a job via sys_object_set_property.
#[test]
fn set_job_oom_kill_bit() {
    let ctx = TestContext::new();

    let root = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(
        root,
        Rights::DEFAULT_JOB | Rights::SET_PROPERTY | Rights::GET_PROPERTY,
    ));

    // ZX_PROP_JOB_KILL_ON_OOM = 15
    let value: u64 = 1;
    ctx.syscall()
        .sys_object_set_property(job_h, 15, &value as *const _ as usize, 8)
        .unwrap();

    let mut read_value: u64 = 0;
    ctx.syscall()
        .sys_object_get_property(job_h, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 1);

    // Reset
    let value: u64 = 0;
    ctx.syscall()
        .sys_object_set_property(job_h, 15, &value as *const _ as usize, 8)
        .unwrap();

    let mut read_value: u64 = 0;
    ctx.syscall()
        .sys_object_get_property(job_h, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 0);
}

/// C++: TEST(JobTest, GetInfoRuntime)
/// Verify that get_info returns valid JobInfo before and after
/// the job is killed.
#[test]
fn get_info_runtime() {
    let ctx = TestContext::new();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (proc_h, _) = create_process(&ctx, job_h, b"info-rt");

    // get_info should succeed on a live job
    let info = get_job_info(&ctx, job_h);
    assert!(!info.exited);

    // Kill the process, then job
    let proc_obj = ctx.proc.get_object::<Process>(proc_h).unwrap();
    proc_obj.exit(0);
    ctx.syscall().sys_task_kill(job_h).unwrap();

    // Job should now be terminated
    let info2 = get_job_info(&ctx, job_h);
    assert!(info2.exited);
}

/// C++: TEST(JobTest, GetInfoRuntimeV1)
/// Verifies that the kill_on_oom property is gettable/settable.
#[test]
fn get_info_runtime_v1() {
    let ctx = TestContext::new();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(
        root,
        Rights::DEFAULT_JOB | Rights::SET_PROPERTY | Rights::GET_PROPERTY,
    ));
    let job_h = create_job(&ctx, root_h);
    // Need SET_PROPERTY on the child job handle too
    let job_obj = ctx.proc.get_object::<Job>(job_h).unwrap();
    let job_h_full = ctx.proc.add_handle(Handle::new(
        job_obj,
        Rights::DEFAULT_JOB | Rights::SET_PROPERTY | Rights::GET_PROPERTY,
    ));

    // Default kill_on_oom should be false
    let mut read_value: u64 = 0;
    ctx.syscall()
        .sys_object_get_property(job_h_full, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 0);

    // Set
    let value: u64 = 1;
    ctx.syscall()
        .sys_object_set_property(job_h_full, 15, &value as *const _ as usize, 8)
        .unwrap();

    // Verify
    ctx.syscall()
        .sys_object_get_property(job_h_full, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 1);

    // get_info should still succeed
    let _info = get_job_info(&ctx, job_h);

    // Reset
    let value: u64 = 0;
    ctx.syscall()
        .sys_object_set_property(job_h_full, 15, &value as *const _ as usize, 8)
        .unwrap();
    ctx.syscall()
        .sys_object_get_property(job_h_full, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 0);
}

/// C++: TEST(JobTest, KillTest)
/// Killing a job with processes: processes are killed, creating new
/// children fails, job is terminated.
#[test]
fn kill_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (_proc1_h, _) = create_process(&ctx, job_h, b"kill1");
    let (_proc2_h, _) = create_process(&ctx, job_h, b"kill2");
    let child_h = create_job(&ctx, job_h);
    let (_proc3_h, _) = create_process(&ctx, child_h, b"kill3");

    // Kill the top job via sys_task_kill
    ctx.syscall().sys_task_kill(job_h).unwrap();

    // Both jobs should be terminated
    let info = get_job_info(&ctx, job_h);
    assert!(info.exited);
    let child_info = get_job_info(&ctx, child_h);
    assert!(child_info.exited);

    // Cannot create new children on the dead job
    let mut fail_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(job_h, 0, UserOutPtr::from(&mut fail_h as *mut _ as usize))
            .unwrap_err(),
        ZxError::BAD_STATE
    );
}

/// C++: TEST(JobTest, OneCriticalProcessKillsOneJobIfRetcodeNonzero)
/// A critical process with retcode_nonzero=true (options=1) only kills
/// the job if the process exits with a non-zero return code.
#[test]
fn one_critical_process_kills_one_job_if_retcode_nonzero() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));

    // Case 1: exit with 0, retcode_nonzero=true (options=1) → job should NOT be killed
    let job1_h = create_job(&ctx, root_h);
    let (proc1_h, _) = create_process(&ctx, job1_h, b"crit-zero");
    // C++: zx_job_set_critical(job1, 1, proc1)  — options=1 means retcode_nonzero
    sc.sys_job_set_critical(job1_h, 1, proc1_h).unwrap();
    let proc1 = ctx.proc.get_object::<Process>(proc1_h).unwrap();
    proc1.exit(0);
    let info1 = get_job_info(&ctx, job1_h);
    assert!(
        !info1.exited,
        "job should NOT be killed when critical process exits with 0 and retcode_nonzero=true"
    );

    // Case 2: exit with non-zero, retcode_nonzero=true → job SHOULD be killed
    let job2_h = create_job(&ctx, root_h);
    let (proc2_h, _) = create_process(&ctx, job2_h, b"crit-nonzero");
    sc.sys_job_set_critical(job2_h, 1, proc2_h).unwrap();
    let proc2 = ctx.proc.get_object::<Process>(proc2_h).unwrap();
    proc2.exit(1);
    let info2 = get_job_info(&ctx, job2_h);
    assert!(
        info2.exited,
        "job should be killed when critical process exits with non-zero and retcode_nonzero=true"
    );
}

/// C++: TEST(JobTest, PolicyInvalidTopicTest)
/// Passing an invalid topic number to sys_job_set_policy returns INVALID_ARGS.
#[test]
fn policy_invalid_topic_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_h = add_root_job(&ctx);
    let job_h = create_job(&ctx, root_h);

    // C++: invalid_topic = 2u (not 0=BASE_V1, 1=TIMER_SLACK, 0x01000000=BASE_V2)
    let some_policy: u32 = 0;
    assert_eq!(
        sc.sys_job_set_policy(
            job_h,
            JOB_POL_RELATIVE,
            2, // invalid topic
            &some_policy as *const _ as usize,
            1,
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidCountTest)
/// Passing count != 1 for timer slack fails with INVALID_ARGS.
#[test]
fn policy_timer_slack_invalid_count_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_h = add_root_job(&ctx);
    let job_h = create_job(&ctx, root_h);

    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 2,
        _padding: 0,
    };

    // Too few (count=0)
    assert_eq!(
        sc.sys_job_set_policy(
            job_h,
            JOB_POL_RELATIVE,
            JOB_POL_TIMER_SLACK,
            &policy as *const _ as usize,
            0,
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Too many (count=2)
    let policies = [policy, policy];
    assert_eq!(
        sc.sys_job_set_policy(
            job_h,
            JOB_POL_RELATIVE,
            JOB_POL_TIMER_SLACK,
            policies.as_ptr() as usize,
            2,
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Just right (count=1)
    sc.sys_job_set_policy(
        job_h,
        JOB_POL_RELATIVE,
        JOB_POL_TIMER_SLACK,
        &policy as *const _ as usize,
        1,
    )
    .unwrap();
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidOptionsTest)
/// Timer slack policy with options != ZX_JOB_POL_RELATIVE (i.e. Absolute)
/// returns INVALID_ARGS.
#[test]
fn policy_timer_slack_invalid_options_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_h = add_root_job(&ctx);
    let job_h = create_job(&ctx, root_h);

    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 2,
        _padding: 0,
    };

    // C++: ZX_JOB_POL_ABSOLUTE is invalid for timer slack
    assert_eq!(
        sc.sys_job_set_policy(
            job_h,
            JOB_POL_ABSOLUTE,
            JOB_POL_TIMER_SLACK,
            &policy as *const _ as usize,
            1,
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // ZX_JOB_POL_RELATIVE is valid
    sc.sys_job_set_policy(
        job_h,
        JOB_POL_RELATIVE,
        JOB_POL_TIMER_SLACK,
        &policy as *const _ as usize,
        1,
    )
    .unwrap();
}

/// C++: TEST(JobTest, PolicyTimerSlackInvalidPolicyTest)
/// A timer slack policy with negative min_slack or invalid mode is
/// rejected with INVALID_ARGS.
#[test]
fn policy_timer_slack_invalid_policy_test() {
    let ctx = TestContext::new();
    let root_h = add_root_job(&ctx);
    let job_h = create_job(&ctx, root_h);

    // Negative min_slack
    let invalid_policy = RawTimerSlackPolicy {
        min_slack: -10_000_000,
        default_mode: 2,
        _padding: 0,
    };
    assert_eq!(
        set_timer_slack_policy(&ctx, job_h, &invalid_policy).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Invalid mode (3 is not a valid Slack variant)
    let invalid_mode_policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 3,
        _padding: 0,
    };
    assert_eq!(
        set_timer_slack_policy(&ctx, job_h, &invalid_mode_policy).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Valid policy should succeed
    let ok_policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 2,
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &ok_policy).unwrap();
}

/// C++: TEST(JobTest, PolicyTimerSlackValid)
/// Setting valid timer slack policies on empty jobs should succeed.
/// Valid modes: Center(0), Early(1), Late(2).
/// Also tests raising and lowering the minimum slack.
#[test]
fn policy_timer_slack_valid() {
    let ctx = TestContext::new();
    let root_h = add_root_job(&ctx);
    let job_h = create_job(&ctx, root_h);

    // All three valid modes: Center(0), Early(1), Late(2).
    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000, // ZX_MSEC(10)
        default_mode: 0,       // Center
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &policy).unwrap();

    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 1, // Early
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &policy).unwrap();

    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000,
        default_mode: 2, // Late
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &policy).unwrap();

    // Raise the minimum.
    let policy = RawTimerSlackPolicy {
        min_slack: 10_000_000_000, // ZX_SEC(10)
        default_mode: 2,           // Late
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &policy).unwrap();

    // Try to lower the minimum, no error.
    let policy = RawTimerSlackPolicy {
        min_slack: 5_000, // ZX_USEC(5)
        default_mode: 0,  // Center
        _padding: 0,
    };
    set_timer_slack_policy(&ctx, job_h, &policy).unwrap();
}

/// C++: TEST(JobTest, WaitTest)
/// A killed job asserts JOB_TERMINATED.
#[test]
fn wait_test() {
    let ctx = TestContext::new();
    let root = Job::root();
    let root_h = ctx.proc.add_handle(Handle::new(root, Rights::DEFAULT_JOB));
    let job_h = create_job(&ctx, root_h);
    let (_proc_h, _) = create_process(&ctx, job_h, b"wait-test");

    // Before kill — not exited
    let info = get_job_info(&ctx, job_h);
    assert!(!info.exited);

    // Kill the job
    ctx.syscall().sys_task_kill(job_h).unwrap();

    // After kill — exited
    let info2 = get_job_info(&ctx, job_h);
    assert!(info2.exited);
}

/// C++: TEST(JobTest, CreateMissingRightsTest)
/// Duplicate a job handle with reduced rights (no WRITE, no MANAGE_JOB),
/// then verify that creating a child job via that handle fails with
/// ACCESS_DENIED.
#[test]
fn create_missing_rights_test() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_job = Job::root();

    // C++: rights = ZX_DEFAULT_JOB_RIGHTS & ~ZX_RIGHT_WRITE & ~ZX_RIGHT_MANAGE_JOB
    let reduced_rights = Rights::DEFAULT_JOB & !Rights::WRITE & !Rights::MANAGE_JOB;
    let handle_value = ctx.proc.add_handle(Handle::new(root_job, reduced_rights));

    // Creating a child should fail with ACCESS_DENIED
    let mut child_h: HandleValue = 0;
    assert_eq!(
        sc.sys_job_create(
            handle_value,
            0,
            UserOutPtr::from(&mut child_h as *mut _ as usize)
        )
        .unwrap_err(),
        ZxError::ACCESS_DENIED
    );
}

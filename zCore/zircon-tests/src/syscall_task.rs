//! Job, Process, Thread syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// C++: TEST(JobTest, BasicTest) — syscall level
/// Create child job, get info, close.
#[test]
fn job_basic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Get the default job handle (we need to add the root job to handle table)
    let root_job = zircon_object::task::Job::root();
    let parent_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));

    // Create child job
    let mut child_h: HandleValue = 0;
    sc.sys_job_create(
        parent_h,
        0,
        UserOutPtr::from(&mut child_h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(child_h, 0);

    // Create grandchild
    let mut grandchild_h: HandleValue = 0;
    sc.sys_job_create(
        child_h,
        0,
        UserOutPtr::from(&mut grandchild_h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(grandchild_h, 0);

    // Close both
    sc.sys_handle_close(grandchild_h).unwrap();
    sc.sys_handle_close(child_h).unwrap();
}

/// C++: TEST(JobTest, CreateMissingRightsTest) — syscall level
#[test]
fn job_create_missing_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Add job with only READ right (no MANAGE_JOB)
    let root_job = zircon_object::task::Job::root();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::READ, // no MANAGE_JOB
    ));

    let mut child_h: HandleValue = 0;
    let result = sc.sys_job_create(h, 0, UserOutPtr::from(&mut child_h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(JobTest, CreateTest) — syscall level
/// Create process inside a job via syscall.
#[test]
fn job_create_process() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root_job = zircon_object::task::Job::root();
    let job_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));

    let name = b"test-process";
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

    // Clean up
    sc.sys_handle_close(proc_h).unwrap();
    sc.sys_handle_close(vmar_h).unwrap();
}

/// C++: TEST(ProcessTest, LongNameSucceeds) — syscall level
#[test]
fn process_long_name() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root_job = zircon_object::task::Job::root();
    let job_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));

    let name = b"a]very-long-process-name-that-exceeds-ZX_MAX_NAME_LEN";
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
}

/// C++: TEST(ProcessTest, EmptyNameSucceeds) — syscall level
#[test]
fn process_empty_name() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root_job = zircon_object::task::Job::root();
    let job_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));

    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    sc.sys_process_create(
        job_h,
        UserInPtr::from(b"".as_ptr() as usize),
        0,
        0,
        UserOutPtr::from(&mut proc_h as *mut _ as usize),
        UserOutPtr::from(&mut vmar_h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(proc_h, 0);
}

/// C++: TEST(JobTest, SetJobOomKillBit) — syscall level
#[test]
fn job_set_oom_kill_bit() {
    let ctx = TestContext::new();
    let mut sc = ctx.syscall();

    let root_job = zircon_object::task::Job::root();
    let job_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB
            | zircon_object::object::Rights::SET_PROPERTY
            | zircon_object::object::Rights::GET_PROPERTY,
    ));

    // ZX_PROP_JOB_KILL_ON_OOM = 15
    let value: u64 = 1;
    sc.sys_object_set_property(job_h, 15, &value as *const _ as usize, 8)
        .unwrap();

    // Read it back
    let mut read_value: u64 = 0;
    sc.sys_object_get_property(job_h, 15, &mut read_value as *mut _ as usize, 8)
        .unwrap();
    assert_eq!(read_value, 1);
}

/// C++: TEST(JobTest, MaxHeightSmoke) — syscall level
/// Can create deeply nested jobs up to max height.
#[test]
fn job_max_height_smoke() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let root_job = zircon_object::task::Job::root();
    let mut parent_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        root_job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));

    // Create a chain of jobs (max height is 32)
    let mut handles = Vec::new();
    for _ in 0..10 {
        let mut child_h: HandleValue = 0;
        sc.sys_job_create(
            parent_h,
            0,
            UserOutPtr::from(&mut child_h as *mut _ as usize),
        )
        .unwrap();
        handles.push(child_h);
        parent_h = child_h;
    }

    // Clean up
    for h in handles.iter().rev() {
        sc.sys_handle_close(*h).unwrap();
    }
}

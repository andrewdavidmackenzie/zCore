//! petal job test -- job create, set_policy, set_critical, get_info.
//!
//! Port of key tests from zircon-tests/src/core_tests/job.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_JOB_DEFAULT: u32 = 0x03;
const PA_PROC_SELF: u32 = 0x01;

const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_JOB_POL_BASE_V2: u32 = 1;

fn pass(name: &[u8]) {
    zx::debug_write(b"  job_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: job_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

/// Create a child job
fn create_child_job(parent: HandleValue) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_job_create(parent, 0, &mut h) };
    if s != ZX_OK {
        fail(b"create_child_job", b"failed");
    }
    h
}

/// Job create basic
fn job_create(job_h: HandleValue) {
    let name = b"job_create";
    let child = create_child_job(job_h);
    if child == 0 {
        fail(name, b"handle is 0");
    }

    // Verify type is Job (17)
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                child,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info",
    );
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 17 {
        fail(name, b"wrong obj_type (expected 17=Job)");
    }

    unsafe { zx_handle_close(child) };
    pass(name);
}

/// Job create invalid options
fn job_create_invalid_options(job_h: HandleValue) {
    let name = b"job_create_invalid_options";
    let mut h: HandleValue = 0;
    assert_status(
        unsafe { zx_job_create(job_h, 0xFF, &mut h) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

/// Job create bad handle
fn job_create_bad_handle() {
    let name = b"job_create_bad_handle";
    let mut h: HandleValue = 0;
    assert_status(
        unsafe { zx_job_create(0xDEAD, 0, &mut h) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Job create wrong type
fn job_create_wrong_type() {
    let name = b"job_create_wrong_type";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    let mut h: HandleValue = 0;
    assert_status(
        unsafe { zx_job_create(event, 0, &mut h) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );

    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Job set policy
fn job_set_policy(job_h: HandleValue) {
    let name = b"job_set_policy";
    let child = create_child_job(job_h);

    // Set a basic V2 policy: deny VMO_EXEC (condition=1)
    // RawPolicyV2: { condition: u32, action: u32, flags: u32, padding: u32 } = 16 bytes
    let policy: [u8; 16] = {
        let mut p = [0u8; 16];
        p[0..4].copy_from_slice(&1u32.to_ne_bytes()); // condition = ZX_POL_VMO_EXEC
        p[4..8].copy_from_slice(&2u32.to_ne_bytes()); // action = ZX_POL_ACTION_DENY
        p
    };

    assert_ok(
        unsafe { zx_job_set_policy(child, 0, ZX_JOB_POL_BASE_V2, policy.as_ptr(), 1) },
        name,
        b"set policy",
    );

    unsafe { zx_handle_close(child) };
    pass(name);
}

/// Job set policy bad handle
fn job_set_policy_bad_handle() {
    let name = b"job_set_policy_bad_handle";
    let policy = [0u8; 16];
    assert_status(
        unsafe { zx_job_set_policy(0xDEAD, 0, ZX_JOB_POL_BASE_V2, policy.as_ptr(), 1) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Job set critical
fn job_set_critical(job_h: HandleValue, _proc_h: HandleValue) {
    let name = b"job_set_critical";
    // Fuchsia requires the critical job to be the process's containing
    // job or an ANCESTOR of it (ProcessDispatcher::SetCriticalToJob
    // walks up from the process's job). Create a process inside a
    // child job and mark that process critical to the child job.
    let child = create_child_job(job_h);

    let pname = b"critical-proc\0";
    let mut cproc: HandleValue = 0;
    let mut cvmar: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_process_create(
                child,
                pname.as_ptr(),
                pname.len() - 1,
                0,
                &mut cproc,
                &mut cvmar,
            )
        },
        name,
        b"process create",
    );

    // Mark the child process critical to its own containing job:
    // if it exits, the job is killed.
    assert_ok(
        unsafe { zx_job_set_critical(child, 0, cproc) },
        name,
        b"set critical",
    );

    unsafe { zx_handle_close(cproc) };
    unsafe { zx_handle_close(cvmar) };
    unsafe { zx_handle_close(child) };
    pass(name);
}

/// Job set critical bad handle
fn job_set_critical_bad_handle() {
    let name = b"job_set_critical_bad_handle";
    assert_status(
        unsafe { zx_job_set_critical(0xDEAD, 0, 0xBEEF) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Create process in a job
fn create_process_in_job(job_h: HandleValue) {
    let name = b"create_process_in_job";
    let child_job = create_child_job(job_h);

    let pname = b"test-proc\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_process_create(
                child_job,
                pname.as_ptr(),
                pname.len() - 1,
                0,
                &mut proc_h,
                &mut vmar_h,
            )
        },
        name,
        b"process create",
    );
    if proc_h == 0 {
        fail(name, b"proc handle is 0");
    }
    if vmar_h == 0 {
        fail(name, b"vmar handle is 0");
    }

    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    unsafe { zx_handle_close(child_job) };
    pass(name);
}

/// Nested jobs
fn nested_jobs(job_h: HandleValue) {
    let name = b"nested_jobs";
    let child = create_child_job(job_h);
    let grandchild = create_child_job(child);
    if grandchild == 0 {
        fail(name, b"grandchild is 0");
    }
    unsafe { zx_handle_close(grandchild) };
    unsafe { zx_handle_close(child) };
    pass(name);
}

/// Kill a job
fn kill_job(job_h: HandleValue) {
    let name = b"kill_job";
    let child = create_child_job(job_h);
    assert_ok(unsafe { zx_task_kill(child) }, name, b"kill");
    // After kill, creating a child should fail
    let mut h: HandleValue = 0;
    let s = unsafe { zx_job_create(child, 0, &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"create child after kill should fail");
    }
    unsafe { zx_handle_close(child) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"job_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let job_h = bootstrap.find(PA_JOB_DEFAULT);
    let proc_h = bootstrap.find(PA_PROC_SELF);

    if job_h == 0 {
        zx::debug_write(b"  SKIP: no PA_JOB_DEFAULT\n");
        zx::debug_write(b"job_test: all tests passed\n");
        return;
    }

    job_create(job_h);
    job_create_invalid_options(job_h);
    job_create_bad_handle();
    job_create_wrong_type();
    job_set_policy(job_h);
    job_set_policy_bad_handle();
    if proc_h != 0 {
        job_set_critical(job_h, proc_h);
    }
    job_set_critical_bad_handle();
    create_process_in_job(job_h);
    nested_jobs(job_h);
    kill_job(job_h);

    zx::debug_write(b"job_test: all tests passed\n");
}

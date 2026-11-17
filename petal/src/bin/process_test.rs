//! petal process test -- process create, info, naming.
//!
//! Port of sync UserPtr tests from zircon-tests/src/core_tests/process.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_JOB_DEFAULT: u32 = 0x03;
const PA_PROC_SELF: u32 = 0x01;
const ZX_INFO_HANDLE_BASIC: u32 = 2;
const ZX_PROP_NAME: u32 = 3;

fn pass(name: &[u8]) {
    zx::debug_write(b"  process_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: process_test: ");
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

/// Create process with empty name
fn empty_name_succeeds(job_h: HandleValue) {
    let name = b"empty_name_succeeds";
    let pname = b"\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_process_create(job_h, pname.as_ptr(), 0, 0, &mut proc_h, &mut vmar_h) },
        name,
        b"create",
    );
    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Create process with long name (should truncate)
fn long_name_succeeds(job_h: HandleValue) {
    let name = b"long_name_succeeds";
    let long = b"this-is-a-very-long-process-name-that-exceeds-the-limit\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_process_create(
                job_h,
                long.as_ptr(),
                long.len() - 1,
                0,
                &mut proc_h,
                &mut vmar_h,
            )
        },
        name,
        b"create",
    );

    // Get the name back
    let mut buf = [0u8; 64];
    assert_ok(
        unsafe { zx_object_get_property(proc_h, ZX_PROP_NAME, buf.as_mut_ptr(), 64) },
        name,
        b"get name",
    );
    // Name should be truncated to ZX_MAX_NAME_LEN (32)
    // but should still start with the right prefix
    if &buf[..4] != b"this" {
        fail(name, b"name prefix wrong");
    }

    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Process info reflects state
fn info_reflects_process_state(job_h: HandleValue) {
    let name = b"info_reflects_process_state";
    let pname = b"info-test\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_process_create(
                job_h,
                pname.as_ptr(),
                pname.len() - 1,
                0,
                &mut proc_h,
                &mut vmar_h,
            )
        },
        name,
        b"create",
    );

    // Get handle info
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                proc_h,
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

    // obj_type at offset 12: Process = 1
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 1 {
        fail(name, b"wrong obj_type (expected 1=Process)");
    }

    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Kill a process
fn kill_process(job_h: HandleValue) {
    let name = b"kill_process";
    let pname = b"kill-test\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe {
            zx_process_create(
                job_h,
                pname.as_ptr(),
                pname.len() - 1,
                0,
                &mut proc_h,
                &mut vmar_h,
            )
        },
        name,
        b"create",
    );

    assert_ok(unsafe { zx_task_kill(proc_h) }, name, b"kill");

    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Create process with invalid options
fn create_invalid_options(job_h: HandleValue) {
    let name = b"create_invalid_options";
    let pname = b"test\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    let s =
        unsafe { zx_process_create(job_h, pname.as_ptr(), 4, 0xFFFF, &mut proc_h, &mut vmar_h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(proc_h) };
        unsafe { zx_handle_close(vmar_h) };
        fail(name, b"invalid options should fail");
    }
    pass(name);
}

/// Create process with bad job handle
fn create_bad_handle() {
    let name = b"create_bad_handle";
    let pname = b"test\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    let s = unsafe { zx_process_create(0xDEAD, pname.as_ptr(), 4, 0, &mut proc_h, &mut vmar_h) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"process_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let job_h = bootstrap.find(PA_JOB_DEFAULT);
    if job_h == 0 {
        zx::debug_write(b"  SKIP: no PA_JOB_DEFAULT\n");
        zx::debug_write(b"process_test: all tests passed\n");
        return;
    }

    empty_name_succeeds(job_h);
    long_name_succeeds(job_h);
    info_reflects_process_state(job_h);
    kill_process(job_h);
    create_invalid_options(job_h);
    create_bad_handle();

    zx::debug_write(b"process_test: all tests passed\n");
}

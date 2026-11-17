//! petal exception extra test -- exception channel creation/validation.
//!
//! Port of zircon-tests/src/exception_extra.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_PROC_SELF: u32 = 0x01;
const PA_JOB_DEFAULT: u32 = 0x03;

fn pass(name: &[u8]) {
    zx::debug_write(b"  exception_extra_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: exception_extra_test: ");
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

/// Exception channel on process
fn create_exception_channel_process(proc_h: HandleValue) {
    let name = b"exc_channel_process";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(proc_h, 0, &mut ch) },
        name,
        b"create",
    );
    unsafe { zx_handle_close(ch) };
    pass(name);
}

/// Debug exception channel on process
fn create_debug_exception_channel(proc_h: HandleValue) {
    let name = b"debug_exc_channel";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(proc_h, 1, &mut ch) },
        name,
        b"create debugger",
    );
    unsafe { zx_handle_close(ch) };
    pass(name);
}

/// Exception channel on job
fn create_exception_channel_job(job_h: HandleValue) {
    let name = b"exc_channel_job";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(job_h, 0, &mut ch) },
        name,
        b"create",
    );
    unsafe { zx_handle_close(ch) };
    pass(name);
}

/// Exception channel on thread
fn create_exception_channel_thread(proc_h: HandleValue) {
    let name = b"exc_channel_thread";
    let mut thread: HandleValue = 0;
    let tname = b"exc-t\0";
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 5, 0, &mut thread) },
        name,
        b"thread create",
    );
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(thread, 0, &mut ch) },
        name,
        b"create",
    );
    unsafe { zx_handle_close(ch) };
    unsafe { zx_handle_close(thread) };
    pass(name);
}

/// Debugger on thread fails
fn debugger_on_thread_fails(proc_h: HandleValue) {
    let name = b"debugger_on_thread_fails";
    let mut thread: HandleValue = 0;
    let tname = b"exc-t2\0";
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 6, 0, &mut thread) },
        name,
        b"thread create",
    );
    let mut ch: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(thread, 1, &mut ch) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    unsafe { zx_handle_close(thread) };
    pass(name);
}

/// Invalid option
fn invalid_option_fails(proc_h: HandleValue) {
    let name = b"invalid_option_fails";
    let mut ch: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(proc_h, 99, &mut ch) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"",
    );
    pass(name);
}

/// Bad handle
fn bad_handle_fails() {
    let name = b"bad_handle_fails";
    let mut ch: HandleValue = 0;
    assert_status(
        unsafe { zx_task_create_exception_channel(0xDEAD, 0, &mut ch) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// Wrong type (event instead of task)
fn wrong_type_fails() {
    let name = b"wrong_type_fails";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );
    let mut ch: HandleValue = 0;
    let s = unsafe { zx_task_create_exception_channel(event, 0, &mut ch) };
    if s == ZX_OK {
        fail(name, b"event should fail");
    }
    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Double bind fails (second exception channel on same task)
fn double_bind_fails(proc_h: HandleValue) {
    let name = b"double_bind_fails";
    let mut ch1: HandleValue = 0;
    let mut ch2: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(proc_h, 0, &mut ch1) },
        name,
        b"first bind",
    );
    assert_status(
        unsafe { zx_task_create_exception_channel(proc_h, 0, &mut ch2) },
        ZX_ERR_ALREADY_BOUND,
        name,
        b"second bind",
    );
    unsafe { zx_handle_close(ch1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"exception_extra_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let proc_h = bootstrap.find(PA_PROC_SELF);
    let job_h = bootstrap.find(PA_JOB_DEFAULT);

    if proc_h == 0 {
        zx::debug_write(b"  SKIP: no PA_PROC_SELF\n");
    } else {
        create_exception_channel_process(proc_h);
        create_debug_exception_channel(proc_h);
        create_exception_channel_thread(proc_h);
        debugger_on_thread_fails(proc_h);
        invalid_option_fails(proc_h);
        double_bind_fails(proc_h);
    }

    if job_h != 0 {
        create_exception_channel_job(job_h);
    }

    bad_handle_fails();
    wrong_type_fails();

    zx::debug_write(b"exception_extra_test: all tests passed\n");
}

//! petal exceptions_arch test -- exception channel creation on different task types.
//!
//! Port of zircon-tests/src/extra_tests/exceptions_arch.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_PROC_SELF: u32 = 0x01;
const PA_JOB_DEFAULT: u32 = 0x03;

fn pass(name: &[u8]) {
    zx::debug_write(b"  exceptions_arch_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: exceptions_arch_test: ");
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

/// Exception channel on process
fn data_abort(proc_h: HandleValue) {
    let name = b"data_abort";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(proc_h, 0, &mut ch) },
        name,
        b"create",
    );
    if ch == 0 {
        fail(name, b"channel is 0");
    }
    unsafe { zx_handle_close(ch) };
    pass(name);
}

/// Exception channel on job
fn instruction_abort(job_h: HandleValue) {
    let name = b"instruction_abort";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(job_h, 0, &mut ch) },
        name,
        b"create",
    );
    if ch == 0 {
        fail(name, b"channel is 0");
    }
    unsafe { zx_handle_close(ch) };
    pass(name);
}

/// Exception channel on thread
fn pc_alignment_fault(proc_h: HandleValue) {
    let name = b"pc_alignment_fault";
    let mut thread_h: HandleValue = 0;
    let thread_name = b"align-thread\0";
    assert_ok(
        unsafe {
            zx_thread_create(
                proc_h,
                thread_name.as_ptr(),
                thread_name.len() - 1,
                0,
                &mut thread_h,
            )
        },
        name,
        b"thread create",
    );

    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(thread_h, 0, &mut ch) },
        name,
        b"create",
    );
    if ch == 0 {
        fail(name, b"channel is 0");
    }
    unsafe { zx_handle_close(ch) };
    unsafe { zx_handle_close(thread_h) };
    pass(name);
}

/// Debugger exception channel on process
fn sp_misalignment(proc_h: HandleValue) {
    let name = b"sp_misalignment";
    let mut ch: HandleValue = 0;
    assert_ok(
        unsafe { zx_task_create_exception_channel(proc_h, 1, &mut ch) },
        name,
        b"create debugger",
    );
    if ch == 0 {
        fail(name, b"channel is 0");
    }
    unsafe { zx_handle_close(ch) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"exceptions_arch_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let proc_h = bootstrap.find(PA_PROC_SELF);
    let job_h = bootstrap.find(PA_JOB_DEFAULT);

    if proc_h == 0 {
        zx::debug_write(b"  SKIP: no PA_PROC_SELF\n");
    } else {
        data_abort(proc_h);
        pc_alignment_fault(proc_h);
        sp_misalignment(proc_h);
    }

    if job_h != 0 {
        instruction_abort(job_h);
    }

    zx::debug_write(b"exceptions_arch_test: all tests passed\n");
}

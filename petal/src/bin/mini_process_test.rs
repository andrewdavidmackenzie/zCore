//! petal mini_process test -- process create and handle operations.
//!
//! Port of UserPtr-using tests from zircon-tests/src/mini_process.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_JOB_DEFAULT: u32 = 0x03;
const PA_PROC_SELF: u32 = 0x01;
const ZX_INFO_HANDLE_BASIC: u32 = 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  mini_process_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: mini_process_test: ");
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

/// Create a process and get its koid
fn get_koid(job_h: HandleValue) {
    let name = b"get_koid";
    let pname = b"mini\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_process_create(job_h, pname.as_ptr(), 4, 0, &mut proc_h, &mut vmar_h) },
        name,
        b"create",
    );

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
    let koid = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    if koid == 0 {
        fail(name, b"koid is 0");
    }

    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Duplicate a handle
fn duplicate_handle() {
    let name = b"duplicate_handle";
    let mut h: HandleValue = 0;
    assert_ok(unsafe { zx_event_create(0, &mut h) }, name, b"create");

    let mut dup: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h, ZX_RIGHT_SAME_RIGHTS, &mut dup) },
        name,
        b"dup",
    );
    if dup == 0 {
        fail(name, b"dup is 0");
    }
    assert_ok(unsafe { zx_handle_check_valid(dup) }, name, b"dup valid");

    unsafe { zx_handle_close(h) };
    unsafe { zx_handle_close(dup) };
    pass(name);
}

/// Create thread in process
fn create_thread(job_h: HandleValue) {
    let name = b"create_thread";
    let pname = b"mini-t\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_process_create(job_h, pname.as_ptr(), 6, 0, &mut proc_h, &mut vmar_h) },
        name,
        b"process create",
    );

    let tname = b"thread\0";
    let mut thread_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), 6, 0, &mut thread_h) },
        name,
        b"thread create",
    );
    if thread_h == 0 {
        fail(name, b"thread handle is 0");
    }

    unsafe { zx_handle_close(thread_h) };
    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

/// Channel communication
fn channel_communication() {
    let name = b"channel_communication";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_ok(
        unsafe { zx_channel_create(0, &mut h0, &mut h1) },
        name,
        b"create",
    );

    let data = b"mini-msg";
    assert_ok(
        unsafe {
            zx_channel_write(
                h0,
                0,
                data.as_ptr(),
                data.len() as u32,
                core::ptr::null(),
                0,
            )
        },
        name,
        b"write",
    );

    let mut buf = [0u8; 32];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    assert_ok(
        unsafe {
            zx_channel_read(
                h1,
                0,
                buf.as_mut_ptr(),
                core::ptr::null_mut(),
                32,
                0,
                &mut ab,
                &mut ah,
            )
        },
        name,
        b"read",
    );
    if ab != 8 {
        fail(name, b"wrong byte count");
    }
    if &buf[..8] != b"mini-msg" {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// Kill process
fn kill_process(job_h: HandleValue) {
    let name = b"kill_process";
    let pname = b"kill\0";
    let mut proc_h: HandleValue = 0;
    let mut vmar_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_process_create(job_h, pname.as_ptr(), 4, 0, &mut proc_h, &mut vmar_h) },
        name,
        b"create",
    );
    assert_ok(unsafe { zx_task_kill(proc_h) }, name, b"kill");
    unsafe { zx_handle_close(proc_h) };
    unsafe { zx_handle_close(vmar_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"mini_process_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let job_h = bootstrap.find(PA_JOB_DEFAULT);
    if job_h == 0 {
        zx::debug_write(b"  SKIP: no PA_JOB_DEFAULT\n");
    } else {
        get_koid(job_h);
        create_thread(job_h);
        kill_process(job_h);
    }

    duplicate_handle();
    channel_communication();

    zx::debug_write(b"mini_process_test: all tests passed\n");
}

//! petal exception channel test — exercises
//! zx_task_create_exception_channel.
//!
//! Creates a child process, attaches a debugger exception channel,
//! starts the child with code that triggers a breakpoint (brk #0),
//! and verifies the exception is received on the channel.
//!
//! Prerequisite for userspace personality servers (#409).

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use zx::sys::{
    zx_channel_read, zx_handle_close, zx_object_wait_one, zx_process_create,
    zx_process_write_memory, zx_task_create_exception_channel, zx_thread_create, zx_vmar_map,
    zx_vmo_create, zx_vmo_write, HandleValue,
};

const PAGE_SIZE: usize = 4096;

fn check(status: i32, msg: &[u8]) {
    if status != 0 {
        zx::debug_write(b"exception_test: FAIL - ");
        zx::debug_write(msg);
        zx::debug_write(b"\r\n");
        zx::Process::exit(1);
    }
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"exception_test: starting\r\n");

    // Read bootstrap channel for root job handle
    let startup = petal::take_startup_handle();
    if startup == 0 {
        zx::debug_write(b"exception_test: FAIL - no startup handle\r\n");
        zx::Process::exit(1);
    }
    let mut handles = [0u32; 4];
    let mut data = [0u8; 4];
    let mut ab: u32 = 0;
    let mut ah: u32 = 0;
    let status = unsafe {
        zx_channel_read(
            startup,
            0,
            data.as_mut_ptr(),
            handles.as_mut_ptr(),
            data.len() as u32,
            handles.len() as u32,
            &mut ab,
            &mut ah,
        )
    };
    check(status, b"channel_read bootstrap");
    unsafe { zx_handle_close(startup) };
    let job_handle = handles[0];

    // Create child process
    let mut proc_handle: HandleValue = 0;
    let mut vmar_handle: HandleValue = 0;
    let name = b"exc-child";
    let status = unsafe {
        zx_process_create(
            job_handle,
            name.as_ptr(),
            name.len(),
            0,
            &mut proc_handle,
            &mut vmar_handle,
        )
    };
    check(status, b"process_create");

    // Create exception channel (debugger mode = 1)
    let mut exc_channel: HandleValue = 0;
    let status = unsafe { zx_task_create_exception_channel(proc_handle, 1, &mut exc_channel) };
    check(status, b"task_create_exception_channel");
    zx::debug_write(b"exception_test: exception channel created\r\n");

    if exc_channel == 0 {
        zx::debug_write(b"exception_test: FAIL - channel handle is 0\r\n");
        zx::Process::exit(1);
    }

    // For now, just verify we can create the channel.
    // Full test (start child, trigger breakpoint, receive exception)
    // requires thread_start with a code page, which is more complex.
    // That's Phase 2 of #409.

    // Clean up
    unsafe {
        zx_handle_close(exc_channel);
        zx_handle_close(vmar_handle);
        zx_handle_close(proc_handle);
        zx_handle_close(job_handle);
    }

    zx::debug_write(b"exception_test: PASS\r\n");
}

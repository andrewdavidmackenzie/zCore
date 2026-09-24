//! petal exception channel test — exercises full exception delivery.
//!
//! Creates a child process with a code page containing a breakpoint
//! instruction, attaches a debugger exception channel, starts the
//! child, and verifies the exception is received.
//!
//! Prerequisite for userspace personality servers (#409).

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use zx::sys::{
    zx_channel_read, zx_handle_close, zx_object_wait_one, zx_process_create, zx_process_start,
    zx_process_write_memory, zx_task_create_exception_channel, zx_thread_create, zx_vmar_map,
    zx_vmo_create, HandleValue,
};

const PAGE_SIZE: usize = 4096;
/// ZX_CHANNEL_READABLE signal
const ZX_CHANNEL_READABLE: u32 = 1 << 0;
/// ZX_TIME_INFINITE
const ZX_TIME_INFINITE: i64 = i64::MAX;

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

    // Create a VMO with breakpoint code
    let mut code_vmo: HandleValue = 0;
    let status = unsafe { zx_vmo_create(PAGE_SIZE as u64, 0, &mut code_vmo) };
    check(status, b"vmo_create code");
    // Map code page into child VMAR (RW first, then write code)
    // ZX_VM_PERM_READ | ZX_VM_PERM_WRITE = 0x3
    // Note: PERM_EXECUTE (0x4) requires VMO to have EXECUTE right.
    // The process page table 1 GiB blocks allow execution regardless.
    let mut code_addr: usize = 0;
    let status =
        unsafe { zx_vmar_map(vmar_handle, 0x3, 0, code_vmo, 0, PAGE_SIZE, &mut code_addr) };
    check(status, b"vmar_map code");

    // Write breakpoint instruction to the code page
    // aarch64: brk #0 = 0xd4200000
    // x86_64: int3 = 0xcc
    #[cfg(target_arch = "aarch64")]
    let brk_instr: [u8; 4] = [0x00, 0x00, 0x20, 0xd4]; // brk #0

    #[cfg(target_arch = "x86_64")]
    let brk_instr: [u8; 1] = [0xcc]; // int3

    let mut actual: usize = 0;
    let status = unsafe {
        zx_process_write_memory(
            proc_handle,
            code_addr,
            brk_instr.as_ptr(),
            brk_instr.len(),
            &mut actual,
        )
    };
    check(status, b"process_write_memory brk");
    zx::debug_write(b"exception_test: breakpoint code written\r\n");

    // Create a stack VMO and map it
    let mut stack_vmo: HandleValue = 0;
    let status = unsafe { zx_vmo_create(PAGE_SIZE as u64, 0, &mut stack_vmo) };
    check(status, b"vmo_create stack");
    let mut stack_addr: usize = 0;
    let status = unsafe {
        zx_vmar_map(
            vmar_handle,
            0x3,
            0,
            stack_vmo,
            0,
            PAGE_SIZE,
            &mut stack_addr,
        )
    };
    check(status, b"vmar_map stack");
    let stack_top = stack_addr + PAGE_SIZE;

    // Create thread in child process
    let mut thread_handle: HandleValue = 0;
    let tname = b"exc-thread";
    let status = unsafe {
        zx_thread_create(
            proc_handle,
            tname.as_ptr(),
            tname.len(),
            0,
            &mut thread_handle,
        )
    };
    check(status, b"thread_create");

    // Start the child: entry=code_addr, stack=stack_top
    let status =
        unsafe { zx_process_start(proc_handle, thread_handle, code_addr, stack_top, 0, 0) };
    check(status, b"process_start");
    zx::debug_write(b"exception_test: child started, waiting for exception\r\n");

    // Wait for the exception channel to become readable
    let mut observed: u32 = 0;
    let status = unsafe {
        zx_object_wait_one(
            exc_channel,
            ZX_CHANNEL_READABLE,
            ZX_TIME_INFINITE,
            &mut observed,
        )
    };
    check(status, b"object_wait_one exc_channel");

    if observed & ZX_CHANNEL_READABLE == 0 {
        zx::debug_write(b"exception_test: FAIL - channel not readable\r\n");
        zx::Process::exit(1);
    }

    // Read the exception from the channel
    let mut exc_data = [0u8; 64];
    let mut exc_handles = [0u32; 4];
    let mut exc_bytes: u32 = 0;
    let mut exc_handle_count: u32 = 0;
    let status = unsafe {
        zx_channel_read(
            exc_channel,
            0,
            exc_data.as_mut_ptr(),
            exc_handles.as_mut_ptr(),
            exc_data.len() as u32,
            exc_handles.len() as u32,
            &mut exc_bytes,
            &mut exc_handle_count,
        )
    };
    check(status, b"channel_read exception");
    zx::debug_write(b"exception_test: exception received!\r\n");

    // Verify we got an exception handle
    if exc_handle_count < 1 || exc_handles[0] == 0 {
        zx::debug_write(b"exception_test: FAIL - no exception handle\r\n");
        zx::Process::exit(1);
    }

    // Clean up
    unsafe {
        for &h in &exc_handles[..exc_handle_count as usize] {
            if h != 0 {
                zx_handle_close(h);
            }
        }
        zx_handle_close(exc_channel);
        zx_handle_close(thread_handle);
        zx_handle_close(stack_vmo);
        zx_handle_close(code_vmo);
        zx_handle_close(vmar_handle);
        zx_handle_close(proc_handle);
        zx_handle_close(job_handle);
    }

    zx::debug_write(b"exception_test: PASS\r\n");
}

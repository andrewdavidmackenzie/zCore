//! petal process memory test — exercises zx_process_read_memory
//! and zx_process_write_memory syscalls.
//!
//! Creates a child process, writes data to its address space,
//! reads it back, and verifies. This is a prerequisite for
//! userspace personality servers (#409).

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use zx::sys::{
    zx_channel_read, zx_handle_close, zx_process_create, zx_process_read_memory,
    zx_process_write_memory, zx_vmar_map, zx_vmo_create, HandleValue,
};

const PAGE_SIZE: usize = 4096;

fn check(status: i32, msg: &[u8]) {
    if status != 0 {
        zx::debug_write(b"process_mem_test: FAIL - ");
        zx::debug_write(msg);
        zx::debug_write(b"\r\n");
        zx::Process::exit(1);
    }
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"process_mem_test: starting\r\n");

    // Read the bootstrap channel to get the root job handle
    let startup = petal::take_startup_handle();
    if startup == 0 {
        zx::debug_write(b"process_mem_test: FAIL - no startup handle\r\n");
        zx::Process::exit(1);
    }

    let mut handles = [0u32; 4];
    let mut data = [0u8; 4];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let status = unsafe {
        zx_channel_read(
            startup,
            0,
            data.as_mut_ptr(),
            handles.as_mut_ptr(),
            data.len() as u32,
            handles.len() as u32,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };
    check(status, b"channel_read bootstrap");
    unsafe { zx_handle_close(startup) };

    let job_handle = handles[0];
    if job_handle == 0 {
        zx::debug_write(b"process_mem_test: FAIL - no job handle\r\n");
        zx::Process::exit(1);
    }

    // Create a child process under the root job
    let mut proc_handle: HandleValue = 0;
    let mut vmar_handle: HandleValue = 0;
    let name = b"test-child";
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
    zx::debug_write(b"process_mem_test: child process created\r\n");

    // Create a VMO and map it into the child's VMAR
    let mut vmo_handle: HandleValue = 0;
    let status = unsafe { zx_vmo_create(PAGE_SIZE as u64, 0, &mut vmo_handle) };
    check(status, b"vmo_create");

    let mut mapped_addr: usize = 0;
    // ZX_VM_PERM_READ | ZX_VM_PERM_WRITE = 0x3
    let status = unsafe {
        zx_vmar_map(
            vmar_handle,
            0x3,
            0,
            vmo_handle,
            0,
            PAGE_SIZE,
            &mut mapped_addr,
        )
    };
    check(status, b"vmar_map");
    zx::debug_write(b"process_mem_test: VMO mapped into child\r\n");

    // Write data to the child's address space
    let test_data = b"Hello from parent!";
    let mut actual: usize = 0;
    let status = unsafe {
        zx_process_write_memory(
            proc_handle,
            mapped_addr,
            test_data.as_ptr(),
            test_data.len(),
            &mut actual,
        )
    };
    check(status, b"process_write_memory");
    if actual != test_data.len() {
        zx::debug_write(b"process_mem_test: FAIL - write actual mismatch\r\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"process_mem_test: data written to child\r\n");

    // Read it back
    let mut read_buf = [0u8; 64];
    actual = 0;
    let status = unsafe {
        zx_process_read_memory(
            proc_handle,
            mapped_addr,
            read_buf.as_mut_ptr(),
            test_data.len(),
            &mut actual,
        )
    };
    check(status, b"process_read_memory");
    if actual != test_data.len() {
        zx::debug_write(b"process_mem_test: FAIL - read actual mismatch\r\n");
        zx::Process::exit(1);
    }
    if &read_buf[..test_data.len()] != test_data {
        zx::debug_write(b"process_mem_test: FAIL - data mismatch\r\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"process_mem_test: data verified\r\n");

    // Clean up
    unsafe {
        zx_handle_close(vmo_handle);
        zx_handle_close(vmar_handle);
        zx_handle_close(proc_handle);
        zx_handle_close(job_handle);
    }

    zx::debug_write(b"process_mem_test: PASS\r\n");
}

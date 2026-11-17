//! petal bad_access test -- verify syscalls reject invalid user pointers.
//!
//! Port of zircon-tests/src/core_tests/bad_access.rs.
//! These tests exercise the kernel's user pointer validation from real
//! user mode — unmapped addresses, kernel-space addresses, null pointers.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

fn pass(name: &[u8]) {
    zx::debug_write(b"  bad_access_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: bad_access_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

/// C++: TEST(BadAccessTest, InvalidMappedAddressFails)
/// Writing from an unmapped address (0x1000) to a channel should fail.
fn invalid_mapped_address_fails() {
    let name = b"invalid_mapped_address_fails";

    // Create a channel pair
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(name, b"channel create");
    }

    // Write from unmapped address (0x1000) with 1 byte
    let unmapped_addr: *const u8 = 4096 as *const u8;
    let s = unsafe { zx_channel_write(h0, 0, unmapped_addr, 1, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"write from unmapped should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(BadAccessTest, KernelMappedAddressChannelWriteFails)
/// Writing from a kernel-space address should fail.
fn kernel_mapped_address_channel_write_fails() {
    let name = b"kernel_mapped_address_channel_write_fails";

    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(name, b"channel create");
    }

    // KERNEL_ASPACE_BASE on x86_64/aarch64 (high canonical addresses)
    let kernel_addr_minus_1 = 0xffff_8000_0000_0000_usize.wrapping_sub(1) as *const u8;
    let s = unsafe { zx_channel_write(h0, 0, kernel_addr_minus_1, 5, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"write crossing kernel boundary should fail");
    }

    let kernel_base = 0xffff_8000_0000_0000_usize as *const u8;
    let s = unsafe { zx_channel_write(h0, 0, kernel_base, 1, core::ptr::null(), 0) };
    if s == ZX_OK {
        fail(name, b"write into kernel space should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(BadAccessTest, NormalMappedAddressChannelWriteSucceeds)
/// Writing from a valid stack address should succeed.
fn normal_mapped_address_channel_write_succeeds() {
    let name = b"normal_mapped_address_channel_write_succeeds";

    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(name, b"channel create");
    }

    // Write from a valid stack pointer
    let stack_data: usize = 4096;
    let s = unsafe {
        zx_channel_write(
            h0,
            0,
            &stack_data as *const usize as *const u8,
            core::mem::size_of::<usize>() as u32,
            core::ptr::null(),
            0,
        )
    };
    if s != ZX_OK {
        fail(name, b"write from valid stack should succeed");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(BadAccessTest, SyscallNumTest)
/// Bad syscall numbers / invalid handle operations.
fn syscall_num_test() {
    let name = b"syscall_num_test";

    // Close a bad handle
    let s = unsafe { zx_handle_close(0xBAD_0001) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }

    let s = unsafe { zx_handle_close(0xBAD_0002) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE 2");
    }

    pass(name);
}

/// C++: TEST(BadAccessTest, ChannelReadHandle)
/// Reading handles into an unmapped address should return an error.
fn channel_read_handle() {
    let name = b"channel_read_handle";

    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(name, b"channel create");
    }

    // Create an event as a transferable handle
    let mut event_h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut event_h) };
    if s != ZX_OK {
        fail(name, b"event create");
    }

    // Write the handle to channel h0
    let handles = [event_h];
    let s = unsafe { zx_channel_write(h0, 0, core::ptr::null(), 0, handles.as_ptr(), 1) };
    if s != ZX_OK {
        fail(name, b"channel write with handle");
    }

    // Read from h1 with handles pointer = unmapped address
    let unmapped_addr: *mut HandleValue = 4096 as *mut HandleValue;
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let s = unsafe {
        zx_channel_read(
            h1,
            0,
            core::ptr::null_mut(),
            unmapped_addr,
            0,
            1,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };
    // Should fail because the handles output pointer is unmapped
    if s == ZX_OK {
        fail(name, b"read with unmapped handles ptr should fail");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"bad_access_test: starting\n");

    invalid_mapped_address_fails();
    kernel_mapped_address_channel_write_fails();
    normal_mapped_address_channel_write_succeeds();
    syscall_num_test();
    channel_read_handle();

    zx::debug_write(b"bad_access_test: all tests passed\n");
}

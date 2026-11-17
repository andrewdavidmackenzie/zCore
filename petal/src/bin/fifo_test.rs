//! petal FIFO test -- exercises FIFO syscalls from user mode.
//!
//! Port of zircon-tests/src/core_tests/fifo.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const ZX_INFO_HANDLE_BASIC: u32 = 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  fifo_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: fifo_test: ");
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

fn assert_err(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s == ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

fn fifo_create(count: usize, elem_size: usize) -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_fifo_create(count, elem_size, 0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"fifo_create", b"failed");
    }
    (h0, h1)
}

/// C++: TEST(FifoTest, NonPowerOfTwoCountSupported)
fn non_power_of_two_count_supported() {
    let name = b"non_power_of_two_count";
    // Count=3 (not a power of 2) should work
    let (h0, h1) = fifo_create(3, 4);
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, SingleElementCapacity)
fn single_element_capacity() {
    let name = b"single_element_capacity";
    let (h0, h1) = fifo_create(1, 4);

    let data = 42u32.to_ne_bytes();
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_fifo_write(h0, 4, data.as_ptr(), 1, &mut actual) },
        name,
        b"write",
    );
    if actual != 1 {
        fail(name, b"write actual != 1");
    }

    // FIFO is full, second write should fail
    let s = unsafe { zx_fifo_write(h0, 4, data.as_ptr(), 1, &mut actual) };
    assert_status(s, ZX_ERR_SHOULD_WAIT, name, b"full fifo");

    // Read should return the value
    let mut buf = [0u8; 4];
    assert_ok(
        unsafe { zx_fifo_read(h1, 4, buf.as_mut_ptr(), 1, &mut actual) },
        name,
        b"read",
    );
    if actual != 1 {
        fail(name, b"read actual != 1");
    }
    if u32::from_ne_bytes(buf) != 42 {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, FifoOrderIsPreserved)
fn fifo_order_is_preserved() {
    let name = b"fifo_order_is_preserved";
    let (h0, h1) = fifo_create(8, 4);

    // Write 4 elements
    for i in 0u32..4 {
        let data = i.to_ne_bytes();
        let mut actual: usize = 0;
        assert_ok(
            unsafe { zx_fifo_write(h0, 4, data.as_ptr(), 1, &mut actual) },
            name,
            b"write",
        );
    }

    // Read them back — order should be preserved (FIFO)
    for i in 0u32..4 {
        let mut buf = [0u8; 4];
        let mut actual: usize = 0;
        assert_ok(
            unsafe { zx_fifo_read(h1, 4, buf.as_mut_ptr(), 1, &mut actual) },
            name,
            b"read",
        );
        if u32::from_ne_bytes(buf) != i {
            fail(name, b"order not preserved");
        }
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, InvalidHandleAndWrongType)
fn invalid_handle_and_wrong_type() {
    let name = b"invalid_handle_and_wrong_type";
    let mut buf = [0u8; 4];
    let mut actual: usize = 0;

    // Read with invalid handle
    assert_status(
        unsafe { zx_fifo_read(0xDEAD, 4, buf.as_mut_ptr(), 1, &mut actual) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"read bad handle",
    );

    // Write with invalid handle
    let data = [0u8; 4];
    assert_status(
        unsafe { zx_fifo_write(0xDEAD, 4, data.as_ptr(), 1, &mut actual) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"write bad handle",
    );

    // Event handle (wrong type) for FIFO read
    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );
    assert_status(
        unsafe { zx_fifo_read(event_h, 4, buf.as_mut_ptr(), 1, &mut actual) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"read wrong type",
    );

    unsafe { zx_handle_close(event_h) };
    pass(name);
}

/// C++: TEST(FifoTest, ReadBadBuffer)
fn read_bad_buffer() {
    let name = b"read_bad_buffer";
    let (h0, h1) = fifo_create(16, 4);

    // Write data so read has something
    let data = 42u32.to_ne_bytes();
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_fifo_write(h0, 4, data.as_ptr(), 1, &mut actual) },
        name,
        b"write",
    );

    // Read with null buffer
    let s = unsafe { zx_fifo_read(h1, 4, core::ptr::null_mut(), 1, &mut actual) };
    assert_err(s, name, b"null buffer should fail");

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, WriteBadBuffer)
fn write_bad_buffer() {
    let name = b"write_bad_buffer";
    let (h0, h1) = fifo_create(16, 4);

    let mut actual: usize = 0;
    let bad_ptr = 4096 as *const u8;
    let s = unsafe { zx_fifo_write(h0, 4, bad_ptr, 1, &mut actual) };
    assert_err(s, name, b"bad buffer should fail");

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, WriteNullBuffer)
fn write_null_buffer() {
    let name = b"write_null_buffer";
    let (h0, h1) = fifo_create(16, 4);

    let mut actual: usize = 0;
    let s = unsafe { zx_fifo_write(h0, 4, core::ptr::null(), 1, &mut actual) };
    assert_err(s, name, b"null buffer should fail");

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, CreateZeroSize)
fn create_zero_size() {
    let name = b"create_zero_size";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    assert_status(
        unsafe { zx_fifo_create(0, 4, 0, &mut h0, &mut h1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"zero count",
    );
    assert_status(
        unsafe { zx_fifo_create(16, 0, 0, &mut h0, &mut h1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"zero elem_size",
    );
    pass(name);
}

/// C++: TEST(FifoTest, EmptyQueueReturnsErrShouldWait)
fn empty_queue_returns_should_wait() {
    let name = b"empty_queue_should_wait";
    let (h0, h1) = fifo_create(16, 4);

    let mut buf = [0u8; 4];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_fifo_read(h1, 4, buf.as_mut_ptr(), 1, &mut actual) },
        ZX_ERR_SHOULD_WAIT,
        name,
        b"",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, EndpointCloseSignalsPeerClosed)
fn endpoint_close_signals_peer_closed() {
    let name = b"endpoint_close_peer_closed";
    let (h0, h1) = fifo_create(16, 4);

    // Close h0
    unsafe { zx_handle_close(h0) };

    // Write to h1 should fail with PEER_CLOSED
    let data = [0u8; 4];
    let mut actual: usize = 0;
    assert_status(
        unsafe { zx_fifo_write(h1, 4, data.as_ptr(), 1, &mut actual) },
        ZX_ERR_PEER_CLOSED,
        name,
        b"",
    );

    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, PartialWriteQueuesElementsThatFit)
fn partial_write_queues_elements_that_fit() {
    let name = b"partial_write";
    let (h0, h1) = fifo_create(4, 4); // capacity 4

    // Write 8 elements — only 4 should fit
    let data: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut actual: usize = 0;
    assert_ok(
        unsafe { zx_fifo_write(h0, 4, data.as_ptr() as *const u8, 8, &mut actual) },
        name,
        b"write",
    );
    if actual != 4 {
        fail(name, b"expected 4 elements written");
    }

    // Read all 4
    let mut buf = [0u32; 4];
    assert_ok(
        unsafe { zx_fifo_read(h1, 4, buf.as_mut_ptr() as *mut u8, 4, &mut actual) },
        name,
        b"read",
    );
    if actual != 4 {
        fail(name, b"expected 4 elements read");
    }
    if buf != [1, 2, 3, 4] {
        fail(name, b"data mismatch");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, ReadWriteKernelAddressBufferReturnsInvalidArgs)
fn read_write_kernel_address_buffer() {
    let name = b"kernel_address_buffer";
    let (h0, h1) = fifo_create(16, 4);

    let kernel_addr = 0xffff_8000_0000_0000_usize as *const u8;
    let mut actual: usize = 0;

    // Write from kernel address should fail
    let s = unsafe { zx_fifo_write(h0, 4, kernel_addr, 1, &mut actual) };
    assert_err(s, name, b"write from kernel addr should fail");

    // Read into kernel address should fail
    let s = unsafe { zx_fifo_read(h1, 4, kernel_addr as *mut u8, 1, &mut actual) };
    assert_err(s, name, b"read into kernel addr should fail");

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, DefaultRightsAndType)
fn default_rights_and_type() {
    let name = b"default_rights_and_type";
    let (h0, h1) = fifo_create(16, 4);

    // Get handle info
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
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

    // obj_type at offset 12: Fifo = 24
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 24 {
        fail(name, b"wrong obj_type (expected 24=Fifo)");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(FifoTest, EndpointsAreRelated)
fn endpoints_are_related() {
    let name = b"endpoints_are_related";
    let (h0, h1) = fifo_create(16, 4);

    let mut buf0 = [0u8; 32];
    let mut buf1 = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;

    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
                ZX_INFO_HANDLE_BASIC,
                buf0.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info h0",
    );
    assert_ok(
        unsafe {
            zx_object_get_info(
                h1,
                ZX_INFO_HANDLE_BASIC,
                buf1.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info h1",
    );

    let koid0 = u64::from_le_bytes(buf0[0..8].try_into().unwrap());
    let koid1 = u64::from_le_bytes(buf1[0..8].try_into().unwrap());
    let related0 = u64::from_le_bytes(buf0[16..24].try_into().unwrap());
    let related1 = u64::from_le_bytes(buf1[16..24].try_into().unwrap());

    if related0 != koid1 {
        fail(name, b"h0.related != h1.koid");
    }
    if related1 != koid0 {
        fail(name, b"h1.related != h0.koid");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"fifo_test: starting\n");

    non_power_of_two_count_supported();
    single_element_capacity();
    fifo_order_is_preserved();
    invalid_handle_and_wrong_type();
    read_bad_buffer();
    write_bad_buffer();
    write_null_buffer();
    create_zero_size();
    empty_queue_returns_should_wait();
    endpoint_close_signals_peer_closed();
    partial_write_queues_elements_that_fit();
    read_write_kernel_address_buffer();
    default_rights_and_type();
    endpoints_are_related();

    zx::debug_write(b"fifo_test: all tests passed\n");
}

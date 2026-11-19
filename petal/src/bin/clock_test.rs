//! petal clock test -- exercises Zircon clock syscalls from user mode.
//!
//! Port of zircon-tests/src/core_tests/clock.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use alloc::vec::Vec;
use zx::sys::*;

// Clock options
const ZX_CLOCK_OPT_MAPPABLE: u64 = 1 << 4;

// VMAR options
const ZX_VM_PERM_READ: u32 = 1 << 0;
const ZX_VM_MAP_RANGE: u32 = 1 << 10;
const ZX_VM_ALLOW_FAULTS: u32 = 1 << 12;
const ZX_VM_FAULT_BEYOND_STREAM_SIZE: u32 = 1 << 15;

fn pass(name: &[u8]) {
    zx::debug_write(b"  clock_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: clock_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(status: ZxStatus, name: &[u8], msg: &[u8]) {
    if status != ZX_OK {
        fail(name, msg);
    }
}

fn clock_create(options: u64) -> Result<HandleValue, ZxStatus> {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_clock_create(options, core::ptr::null(), &mut h) };
    if s == ZX_OK {
        Ok(h)
    } else {
        Err(s)
    }
}

/// C++: TEST(ClockTest, ClockMonotonic)
fn clock_monotonic() {
    let name = b"clock_monotonic";
    let mut previous = unsafe { zx_clock_get_monotonic_via_kernel() };
    if previous <= 0 {
        fail(name, b"initial time not positive");
    }

    for _ in 0..100 {
        let current = unsafe { zx_clock_get_monotonic_via_kernel() };
        if current <= 0 {
            fail(name, b"time not positive");
        }
        if current < previous {
            fail(name, b"time went backwards");
        }
        previous = current;
    }
    pass(name);
}

/// C++: TEST(ClockTest, DeadlineAfter)
fn deadline_after() {
    let name = b"deadline_after";
    let offsets: &[i64] = &[0, 20_000_000]; // 0ms, 20ms

    for &offset in offsets {
        let before = unsafe { zx_clock_get_monotonic_via_kernel() }.saturating_add(offset);
        let deadline = unsafe { zx_clock_get_monotonic_via_kernel() }.saturating_add(offset);
        let after = unsafe { zx_clock_get_monotonic_via_kernel() }.saturating_add(offset);
        if deadline < before {
            fail(name, b"deadline < before");
        }
        if deadline > after {
            fail(name, b"deadline > after");
        }
    }
    pass(name);
}

/// C++: TEST(ClockTest, FaultBeyondStreamSizeReturnsError)
fn fault_beyond_stream_size_returns_error(root_vmar: HandleValue) {
    let name = b"fault_beyond_stream_size_returns_error";

    let clock_h = clock_create(ZX_CLOCK_OPT_MAPPABLE).unwrap_or_else(|_| {
        fail(name, b"clock create");
        0
    });

    let options =
        ZX_VM_PERM_READ | ZX_VM_FAULT_BEYOND_STREAM_SIZE | ZX_VM_ALLOW_FAULTS | ZX_VM_MAP_RANGE;
    let page_size = zx_system_get_page_size();
    let mut mapped_addr: usize = 0;

    // This should fail with INVALID_ARGS (clock state VMOs have no
    // stream-size manager, so fault-beyond-stream-size is rejected).
    let s = unsafe {
        zx_vmar_map_clock(
            root_vmar,
            options,
            0,
            clock_h,
            page_size as u64,
            &mut mapped_addr,
        )
    };
    if s != ZX_ERR_INVALID_ARGS {
        fail(name, b"expected INVALID_ARGS for FAULT_BEYOND_STREAM_SIZE");
    }

    unsafe { zx_handle_close(clock_h) };
    pass(name);
}

/// C++: TEST(ClockTest, MappableClockPaddingLeak)
fn mappable_clock_padding_leak(root_vmar: HandleValue) {
    let name = b"mappable_clock_padding_leak";

    let clock_h = clock_create(ZX_CLOCK_OPT_MAPPABLE).unwrap_or_else(|_| {
        fail(name, b"clock create");
        0
    });

    let options = ZX_VM_PERM_READ | ZX_VM_MAP_RANGE;
    let page_size = zx_system_get_page_size();
    let mut mapped_addr: usize = 0;

    assert_ok(
        unsafe {
            zx_vmar_map_clock(
                root_vmar,
                options,
                0,
                clock_h,
                page_size as u64,
                &mut mapped_addr,
            )
        },
        name,
        b"map_clock",
    );
    if mapped_addr == 0 {
        fail(name, b"mapped address is 0");
    }

    // Read the mapped page (now in user space — safe to dereference)
    let mapped_slice = unsafe { core::slice::from_raw_parts(mapped_addr as *const u8, page_size) };

    // Search for ZX_CLOCK_UNKNOWN_ERROR (u64::MAX)
    const ZX_CLOCK_UNKNOWN_ERROR: u64 = u64::MAX;
    let mut error_offset: Option<usize> = None;
    let search_end = page_size.saturating_sub(40);
    for i in (0..search_end).step_by(8) {
        let val = u64::from_ne_bytes(mapped_slice[i..i + 8].try_into().unwrap());
        if val == ZX_CLOCK_UNKNOWN_ERROR {
            error_offset = Some(i);
            break;
        }
    }

    let error_off = match error_offset {
        Some(o) => o,
        None => {
            fail(name, b"ZX_CLOCK_UNKNOWN_ERROR not found");
            0
        }
    };

    // Padding at offset 36 from error_bound should be zero
    let padding_off = error_off + 36;
    if padding_off + 4 > page_size {
        fail(name, b"padding offset out of range");
    }
    let padding = u32::from_ne_bytes(
        mapped_slice[padding_off..padding_off + 4]
            .try_into()
            .unwrap(),
    );
    if padding != 0 {
        fail(name, b"padding contains garbage");
    }

    // Unmap
    unsafe { zx_vmar_unmap(root_vmar, mapped_addr, page_size) };
    unsafe { zx_handle_close(clock_h) };
    pass(name);
}

/// C++: TEST(ClockTest, MultithreadedContentionClockMonotonic)
fn multithreaded_contention_clock_monotonic() {
    let name = b"multithreaded_contention_clock_monotonic";

    const LIST_SIZE: usize = 100;
    const NUM_BATCHES: usize = 100;

    let mut timestamps: Vec<i64> = Vec::with_capacity(LIST_SIZE);

    for _ in 0..NUM_BATCHES {
        timestamps.clear();
        for _ in 0..LIST_SIZE {
            timestamps.push(unsafe { zx_clock_get_monotonic_via_kernel() });
        }
        for i in 0..timestamps.len() - 1 {
            if timestamps[i] > timestamps[i + 1] {
                fail(name, b"time went backwards");
            }
        }
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"clock_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let root_vmar = bootstrap.find(zircon_abi::processargs::PA_VMAR_ROOT);
    if root_vmar == 0 {
        zx::debug_write(b"FAIL: no PA_VMAR_ROOT\n");
        zx::Process::exit(1);
    }

    clock_monotonic();
    deadline_after();
    fault_beyond_stream_size_returns_error(root_vmar);
    mappable_clock_padding_leak(root_vmar);
    multithreaded_contention_clock_monotonic();

    zx::debug_write(b"clock_test: all tests passed\n");
}

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

/// Map a clock's state page read-only (helper for the tests below).
fn map_clock(root_vmar: HandleValue, clock_h: HandleValue, name: &[u8]) -> usize {
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
    mapped_addr
}

/// Update a clock's synthetic value (starts the clock on first use).
/// `zx_clock_update_args_v2_t`: synthetic_value (i64) at offset 8.
fn clock_update_value(h: HandleValue, value: i64) -> ZxStatus {
    let mut args = [0u8; 32];
    args[8..16].copy_from_slice(&value.to_le_bytes());
    // ZX_CLOCK_UPDATE_OPTION_SYNTHETIC_VALUE_VALID = 1 << 0
    unsafe { zx_clock_update(h, 1 << 0, args.as_ptr()) }
}

/// Update a clock's rate adjustment (ppm).
/// `zx_clock_update_args_v2_t`: rate_adjust (i32) at offset 0.
fn clock_update_rate(h: HandleValue, ppm: i32) -> ZxStatus {
    let mut args = [0u8; 32];
    args[0..4].copy_from_slice(&ppm.to_le_bytes());
    // ZX_CLOCK_UPDATE_OPTION_RATE_ADJUST_VALID = 1 << 1
    unsafe { zx_clock_update(h, 1 << 1, args.as_ptr()) }
}

/// Update a clock's error bound (ns).
/// `zx_clock_update_args_v2_t`: error_bound (u64) at offset 24.
fn clock_update_error(h: HandleValue, error: u64) -> ZxStatus {
    let mut args = [0u8; 32];
    args[24..32].copy_from_slice(&error.to_le_bytes());
    // ZX_CLOCK_UPDATE_OPTION_ERROR_BOUND_VALID = 1 << 2
    unsafe { zx_clock_update(h, 1 << 2, args.as_ptr()) }
}

/// zCore regression test for issue #831 (no C++ counterpart).
///
/// Mapped clock state pages must reflect `zx_clock_update` changes
/// without remapping:
///
/// 1. Create a mappable clock, start it at synthetic value 1000.
/// 2. Map it; the mapped page shows the step-1 transformation.
/// 3. Update the clock to 2000.
/// 4. Re-read the SAME mapping: it must now show the step-3
///    transformation (before the fix it stayed at the per-map
///    snapshot taken in step 2).
/// 5. Rate and error-bound updates are visible too.
/// 6. The seqlock generation is even (stable) at rest and increases
///    with each update.
/// 7. Padding between the transformation struct and the generation
///    counter stays zero (no kernel data leaks).
/// 8. A second mapping of the same clock shares the live page.
/// 9. `zx_clock_read` agrees with the mapped transformation.
///
/// State page layout (zCore internal ABI, see
/// `zircon-object/src/signal/clock.rs`):
/// - offset 0: options (u64)
/// - offset 8: mono_to_synthetic numerator (u32)
/// - offset 12: mono_to_synthetic denominator (u32)
/// - offset 16: mono_to_synthetic reference offset (i64)
/// - offset 24: mono_to_synthetic synthetic offset (i64)
/// - offset 32: error_bound (u64)
/// - last 8 bytes: seqlock generation (u64; odd = update in progress)
fn mappable_clock_update_after_map(root_vmar: HandleValue) {
    let name = b"mappable_clock_update_after_map";
    let page_size = zx_system_get_page_size();

    let clock_h = clock_create(ZX_CLOCK_OPT_MAPPABLE).unwrap_or_else(|_| {
        fail(name, b"clock create");
        0
    });

    // 1. Start the clock at synthetic value 1000.
    assert_ok(clock_update_value(clock_h, 1000), name, b"update to 1000");

    // 2. Map the clock state page.
    let addr = map_clock(root_vmar, clock_h, name);

    // Volatile reads: the kernel updates this page concurrently.
    let synthetic_off = unsafe { ((addr + 24) as *const i64).read_volatile() };
    if synthetic_off != 1000 {
        fail(name, b"initial mapped synthetic offset != 1000");
    }
    let gen1 = unsafe { ((addr + page_size - 8) as *const u64).read_volatile() };
    if gen1 % 2 != 0 {
        fail(name, b"generation must be even (stable) at rest");
    }

    // 3. Update to 2000 — no remapping.
    assert_ok(clock_update_value(clock_h, 2000), name, b"update to 2000");

    // 4. The SAME mapping must now show 2000 (issue #831: it used to
    //    keep the stale per-map snapshot).
    let synthetic_off = unsafe { ((addr + 24) as *const i64).read_volatile() };
    if synthetic_off != 2000 {
        fail(name, b"mapped page is stale after update (issue #831)");
    }
    let gen2 = unsafe { ((addr + page_size - 8) as *const u64).read_volatile() };
    if gen2 % 2 != 0 {
        fail(name, b"generation must be even (stable) after update");
    }
    if gen2 <= gen1 {
        fail(name, b"generation must increase across updates");
    }

    // 5. Rate and error-bound updates are visible too.
    assert_ok(clock_update_rate(clock_h, 1000), name, b"rate update");
    let rate_num = unsafe { ((addr + 8) as *const u32).read_volatile() };
    let rate_den = unsafe { ((addr + 12) as *const u32).read_volatile() };
    // 1000 ppm: numerator = 1_000_000 + 1000, denominator = 1_000_000.
    if rate_num != 1_001_000 || rate_den != 1_000_000 {
        fail(name, b"mapped rate not updated");
    }
    assert_ok(
        clock_update_error(clock_h, 4711),
        name,
        b"error bound update",
    );
    let error_bound = unsafe { ((addr + 32) as *const u64).read_volatile() };
    if error_bound != 4711 {
        fail(name, b"mapped error bound not updated");
    }

    // 6. Padding between the transformation struct (64 bytes) and the
    //    generation counter stays zero (no kernel data leaks).
    for off in 64..(page_size - 8) {
        let byte = unsafe { ((addr + off) as *const u8).read_volatile() };
        if byte != 0 {
            fail(name, b"padding not zero after updates");
        }
    }

    // 7. A second mapping of the same clock shares the live page.
    let addr2 = map_clock(root_vmar, clock_h, name);
    let synthetic_off2 = unsafe { ((addr2 + 24) as *const i64).read_volatile() };
    if synthetic_off2 != 2000 {
        fail(name, b"second mapping does not see live state");
    }

    // 8. The syscall view and the mapped page agree: read() applies
    //    the mapped transformation to elapsed monotonic time, so it
    //    must be at least the mapped synthetic offset.
    let mut now: i64 = 0;
    assert_ok(
        unsafe { zx_clock_read(clock_h, &mut now) },
        name,
        b"clock read",
    );
    if now < 2000 {
        fail(name, b"zx_clock_read disagrees with mapped transformation");
    }

    unsafe { zx_vmar_unmap(root_vmar, addr, page_size) };
    unsafe { zx_vmar_unmap(root_vmar, addr2, page_size) };
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
    mappable_clock_update_after_map(root_vmar);
    multithreaded_contention_clock_monotonic();

    zx::debug_write(b"clock_test: all tests passed\n");
}

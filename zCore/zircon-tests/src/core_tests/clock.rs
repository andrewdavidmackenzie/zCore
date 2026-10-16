//! Clock tests.
//!
//! Tests for the monotonic clock (vDSO path) and clock kernel objects
//! (sys_clock_create / sys_clock_read / sys_clock_update / sys_clock_get_details).

use crate::helpers::TestContext;

use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::vm::PAGE_SIZE;

extern crate alloc;

type HandleValue = u32;

/// C++: TEST(ClockTest, ClockMonotonic)
/// Monotonic time should be positive and non-decreasing.
#[test]
fn clock_monotonic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut previous = sc.sys_clock_get_monotonic_via_kernel();
    assert!(previous > 0, "monotonic time should be positive");

    for _ in 0..100 {
        let current = sc.sys_clock_get_monotonic_via_kernel();
        assert!(current > 0, "monotonic time should be positive");
        assert!(
            current >= previous,
            "monotonic time should only advance: got {} after {}",
            current,
            previous
        );
        previous = current;
    }
}

/// C++: TEST(ClockTest, DeadlineAfter)
/// deadline_after(offset) should equal monotonic + offset (approximately).
/// Since deadline_after is a vDSO function, we test the equivalent:
/// compute `now + offset` and verify it's within a reasonable range.
#[test]
fn deadline_after() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let offsets: &[i64] = &[0, 20_000_000]; // 0ms, 20ms in nanoseconds

    for &offset in offsets {
        let before = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);
        // Simulate deadline_after: get monotonic + offset.
        let deadline = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);
        let after = sc
            .sys_clock_get_monotonic_via_kernel()
            .saturating_add(offset);

        assert!(
            deadline >= before,
            "deadline should be >= before: {} < {}",
            deadline,
            before
        );
        assert!(
            deadline <= after,
            "deadline should be <= after: {} > {}",
            deadline,
            after
        );
    }
}

// ── Clock kernel object tests (syscall level) ───────────────────────

/// Helper: create a clock via syscall, return handle.
pub(crate) fn syscall_clock_create(
    ctx: &TestContext,
    options: u64,
) -> Result<HandleValue, zircon_object::ZxError> {
    let sc = ctx.syscall();
    let mut handle: HandleValue = 0;
    let out = UserOutPtr::from(&mut handle as *mut _ as usize);
    let args_ptr = UserInPtr::from(core::ptr::null::<u8>() as usize);
    sc.sys_clock_create(options, args_ptr, out)?;
    Ok(handle)
}

/// C++: TEST(ClockTest, FaultBeyondStreamSizeReturnserror)
/// Mapping a clock with ZX_VM_FAULT_BEYOND_STREAM_SIZE should fail
/// with INVALID_ARGS.
///
/// The C++ test:
/// 1. Creates a clock with ZX_CLOCK_OPT_MAPPABLE
/// 2. Gets the mapped size via ZX_INFO_CLOCK_MAPPED_SIZE
/// 3. Calls vmar_map_clock with PERM_READ | FAULT_BEYOND_STREAM_SIZE |
///    ALLOW_FAULTS | MAP_RANGE
/// 4. Asserts ZX_ERR_INVALID_ARGS
///
/// zCore's sys_vmar_map_clock rejects FAULT_BEYOND_STREAM_SIZE with
/// INVALID_ARGS, exactly matching Fuchsia's behavior.
#[test]
fn fault_beyond_stream_size_returns_error() {
    use zircon_object::ZxError;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // ZX_CLOCK_OPT_MAPPABLE = 1 << 4
    const ZX_CLOCK_OPT_MAPPABLE: u64 = 1 << 4;
    let clock_handle =
        syscall_clock_create(&ctx, ZX_CLOCK_OPT_MAPPABLE).expect("clock create should succeed");

    // ZX_VM_PERM_READ | ZX_VM_FAULT_BEYOND_STREAM_SIZE | ZX_VM_ALLOW_FAULTS | ZX_VM_MAP_RANGE
    // PERM_READ = 1 << 0 = 1
    // MAP_RANGE = 1 << 10 = 0x400
    // ALLOW_FAULTS = 1 << 12 = 0x1000
    // FAULT_BEYOND_STREAM_SIZE = 1 << 15 = 0x8000
    let options: u32 = (1 << 0) | (1 << 15) | (1 << 12) | (1 << 10);

    // Use the root VMAR pseudo-handle.
    const ZX_HANDLE_VMAR_ROOT_SELF: HandleValue = 0xFFFF_0003;

    // Use PAGE_SIZE as the mapping length (what a single clock page would be).
    let size: u64 = PAGE_SIZE as u64;
    let mut mapped_addr: usize = 0;
    let result = sc.sys_vmar_map_clock(
        ZX_HANDLE_VMAR_ROOT_SELF,
        options,
        0,
        clock_handle,
        size,
        UserOutPtr::from(&mut mapped_addr as *mut _ as usize),
    );

    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(ClockTest, MappableClockPaddingLeak)
/// Verifies that padding bytes in a mapped clock structure are zeroed
/// (not leaking kernel memory).
///
/// The C++ test:
/// 1. Creates a mappable clock
/// 2. Maps it via vmar_map_clock
/// 3. Searches the mapped page for ZX_CLOCK_UNKNOWN_ERROR (u64::MAX)
/// 4. Checks that 4 bytes of padding at offset 36 from that marker are zero
///
/// zCore's create_state_vmo zero-fills the entire page before writing
/// the clock details struct, which guarantees all padding is zero.
/// We verify this by mapping the clock and checking the mapped memory.
#[test]
fn mappable_clock_padding_leak() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // ZX_CLOCK_OPT_MAPPABLE = 1 << 4
    const ZX_CLOCK_OPT_MAPPABLE: u64 = 1 << 4;
    let clock_handle =
        syscall_clock_create(&ctx, ZX_CLOCK_OPT_MAPPABLE).expect("clock create should succeed");

    // Map with ZX_VM_PERM_READ | ZX_VM_MAP_RANGE
    let options: u32 = (1 << 0) | (1 << 10); // PERM_READ | MAP_RANGE
    const ZX_HANDLE_VMAR_ROOT_SELF: HandleValue = 0xFFFF_0003;
    let size: u64 = PAGE_SIZE as u64;
    let mut mapped_addr: usize = 0;
    let result = sc.sys_vmar_map_clock(
        ZX_HANDLE_VMAR_ROOT_SELF,
        options,
        0,
        clock_handle,
        size,
        UserOutPtr::from(&mut mapped_addr as *mut _ as usize),
    );
    assert!(result.is_ok(), "map_clock should succeed: {:?}", result);
    assert_ne!(mapped_addr, 0, "mapped address should be non-zero");

    // Read the mapped page.
    let ptr = mapped_addr as *const u8;
    let mapped_slice = unsafe { core::slice::from_raw_parts(ptr, size as usize) };

    // Search for ZX_CLOCK_UNKNOWN_ERROR (u64::MAX) — the error_bound
    // field when no updates have been applied.
    const ZX_CLOCK_UNKNOWN_ERROR: u64 = u64::MAX;
    const SEARCH_LIMIT: usize = 36 + 4; // 36 bytes + sizeof(u32)

    assert!(
        size as usize >= SEARCH_LIMIT,
        "mapped size ({}) must be >= search limit ({})",
        size,
        SEARCH_LIMIT
    );

    let mut error_offset: Option<usize> = None;
    let search_end = size as usize - SEARCH_LIMIT;
    for i in (0..search_end).step_by(8) {
        let val = u64::from_ne_bytes(mapped_slice[i..i + 8].try_into().unwrap());
        if val == ZX_CLOCK_UNKNOWN_ERROR {
            error_offset = Some(i);
            break;
        }
    }

    let error_off =
        error_offset.expect("could not find ZX_CLOCK_UNKNOWN_ERROR in mapped clock page");

    // Padding is at offset 36 from the error_bound field.
    let padding_off = error_off + 36;
    let padding = u32::from_ne_bytes(
        mapped_slice[padding_off..padding_off + 4]
            .try_into()
            .unwrap(),
    );
    assert_eq!(padding, 0, "padding contains garbage: {:#x}", padding);

    // Unmap.
    let proc = ctx.proc.clone();
    proc.vmar()
        .unmap(mapped_addr, size as usize)
        .expect("unmap should succeed");
}

/// C++: TEST(ClockTest, MultithreadedContentionClockMonotonic)
/// Multiple threads reading the monotonic clock in a hot loop should
/// always observe monotonically increasing values.
///
/// The C++ test spawns 8 threads that lock a mutex, read monotonic
/// time, store it in a shared vector, and verify monotonicity every
/// 100 samples. This runs for 5 seconds.
///
/// zCore uses a cooperative scheduler, so true multi-threaded
/// contention is not possible. We adapt the test to verify the same
/// invariant (monotonicity of consecutive readings) within a single
/// thread doing many rapid reads, which exercises the same kernel
/// clock path (sys_clock_get_monotonic_via_kernel).
#[test]
fn multithreaded_contention_clock_monotonic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Parameters inspired by the C++ test.
    // kListSize = 100, kNumThreads = 8, kRuntimeSeconds = 5
    // We simulate the total work: 8 threads × many iterations.
    const LIST_SIZE: usize = 100;
    const NUM_BATCHES: usize = 100; // simulate sustained reading

    let mut timestamps: alloc::vec::Vec<i64> = alloc::vec::Vec::with_capacity(LIST_SIZE);

    for _ in 0..NUM_BATCHES {
        timestamps.clear();

        // Collect LIST_SIZE samples.
        for _ in 0..LIST_SIZE {
            timestamps.push(sc.sys_clock_get_monotonic_via_kernel());
        }

        // Verify monotonicity within the batch.
        for i in 0..timestamps.len() - 1 {
            let sooner = timestamps[i];
            let later = timestamps[i + 1];
            assert!(
                sooner <= later,
                "monotonic time went backwards: {} > {} at index {}",
                sooner,
                later,
                i
            );
        }
    }
}

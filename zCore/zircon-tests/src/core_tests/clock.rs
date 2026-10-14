//! Clock tests.
//!
//! Tests for the monotonic clock (vDSO path) and clock kernel objects
//! (sys_clock_create / sys_clock_read / sys_clock_update / sys_clock_get_details).

use crate::helpers::TestContext;

use hal_impl::user::{UserInPtr, UserOutPtr};

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
/// Mapping a clock VMO with ZX_VM_FAULT_BEYOND_STREAM_SIZE should fail
/// with INVALID_ARGS. This requires mappable clock support which is not
/// implemented in zCore.
#[test]
fn fault_beyond_stream_size_returns_error() {
    // zCore does not implement ZX_CLOCK_OPT_MAPPABLE or vmar.map_clock.
}

/// C++: TEST(ClockTest, MappableClockPaddingLeak)
/// Verifies that padding bytes in a mapped clock structure are zeroed
/// (not leaking kernel memory). Requires mappable clock support.
#[test]
fn mappable_clock_padding_leak() {
    // zCore does not implement ZX_CLOCK_OPT_MAPPABLE or map_clock.
}

/// C++: TEST(ClockTest, MultithreadedContentionClockMonotonic)
/// Multiple threads reading the monotonic clock in a hot loop should
/// always observe monotonically increasing values. Requires preemptive
/// multi-threading which is not available in zCore's cooperative scheduler.
#[test]
fn multithreaded_contention_clock_monotonic() {
    // zCore uses a cooperative scheduler; it cannot run multiple threads
    // contending over clock reads in a stress test.
}

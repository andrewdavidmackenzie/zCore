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
fn syscall_clock_create(
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

/// Create an auto-start clock and read it.
#[test]
fn clock_create_auto_start_and_read() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let handle =
        syscall_clock_create(&ctx, zircon_object::signal::ZX_CLOCK_OPT_AUTO_START).unwrap();

    // Read should succeed and return a non-negative value.
    let mut value: u64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    sc.sys_clock_read(handle, out).unwrap();
    // Auto-started clock reads current monotonic time, should be > 0.
    assert!(value > 0, "auto-start clock should read > 0, got {}", value);
}

/// Create a non-auto-start clock — read before start should fail.
#[test]
fn clock_read_before_start_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let handle = syscall_clock_create(&ctx, 0).unwrap();

    let mut value: u64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    let result = sc.sys_clock_read(handle, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_STATE),
        "reading un-started clock should return BAD_STATE"
    );
}

/// Update a clock to start it, then read.
#[test]
fn clock_update_starts_clock() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let handle = syscall_clock_create(&ctx, 0).unwrap();

    // Build update args: set value to 1000.
    // Option bit 0 = set synthetic_value.
    let mut args = [0u8; 32];
    let synthetic_value: i64 = 1000;
    args[8..16].copy_from_slice(&synthetic_value.to_le_bytes());

    let args_ptr = UserInPtr::from(args.as_ptr() as usize);
    sc.sys_clock_update(handle, 1, args_ptr).unwrap(); // option 1 = OPT_VALUE

    // Now read should succeed.
    let mut value: u64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    sc.sys_clock_read(handle, out).unwrap();
    // Value should be >= 1000 (synthetic_value + elapsed time).
    assert!(
        value >= 1000,
        "clock value after update should be >= 1000, got {}",
        value
    );
}

/// Get clock details.
#[test]
fn clock_get_details() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let options = zircon_object::signal::ZX_CLOCK_OPT_AUTO_START
        | zircon_object::signal::ZX_CLOCK_OPT_MONOTONIC;
    let handle = syscall_clock_create(&ctx, options).unwrap();

    let mut details = [0u8; 64];
    let out = UserOutPtr::from(details.as_mut_ptr() as usize);
    sc.sys_clock_get_details(handle, 0, out).unwrap();

    // First 8 bytes = options.
    let read_options = u64::from_le_bytes(details[0..8].try_into().unwrap());
    assert_eq!(
        read_options, options,
        "details options should match creation options"
    );

    // Rate numerator (offset 8) defaults to 1.
    let rate_num = u32::from_le_bytes(details[8..12].try_into().unwrap());
    assert_eq!(rate_num, 1);

    // Rate denominator (offset 12) defaults to 1.
    let rate_den = u32::from_le_bytes(details[12..16].try_into().unwrap());
    assert_eq!(rate_den, 1);
}

/// Clock adjust with root resource — UTC offset.
#[test]
fn clock_adjust_utc() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let resource_h = ctx.add_root_resource();

    // Adjust UTC clock offset.
    sc.sys_clock_adjust(resource_h, 1, 42).unwrap(); // clock_id 1 = ZX_CLOCK_UTC

    // Adjusting MONOTONIC should be denied.
    let result = sc.sys_clock_adjust(resource_h, 0, 0); // clock_id 0 = ZX_CLOCK_MONOTONIC
    assert_eq!(result, Err(zircon_object::ZxError::ACCESS_DENIED));

    // Invalid clock_id.
    let result = sc.sys_clock_adjust(resource_h, 99, 0);
    assert_eq!(result, Err(zircon_object::ZxError::INVALID_ARGS));
}

/// Clock adjust without resource fails.
#[test]
fn clock_adjust_needs_resource() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // No resource handle → BAD_HANDLE.
    let result = sc.sys_clock_adjust(0xDEAD, 1, 0);
    assert_eq!(result, Err(zircon_object::ZxError::BAD_HANDLE));
}

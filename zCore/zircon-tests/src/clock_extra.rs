//! Additional zCore-specific tests for clock functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use crate::core_tests::clock::{syscall_clock_create};

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


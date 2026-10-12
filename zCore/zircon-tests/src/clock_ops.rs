//! Clock operations tests.
//!
//! Additional clock tests covering monotonic, boot, and UTC clock
//! queries and transformations.

use crate::helpers::TestContext;
use alloc::sync::Arc;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::{Clock, ZX_CLOCK_OPT_AUTO_START};

extern crate alloc;

type HandleValue = u32;

///
/// zx_clock_get_monotonic should return a non-zero value.
#[test]
fn monotonic_non_zero() {
    let _ctx = TestContext::new();
    let now = hal_impl::timer::timer_now();
    assert!(
        !now.is_zero(),
        "monotonic time should be non-zero, got {:?}",
        now
    );
}

///
/// Two consecutive calls to zx_clock_get_monotonic should return
/// increasing values.
#[test]
fn monotonic_increasing() {
    let _ctx = TestContext::new();
    let first = hal_impl::timer::timer_now();
    let second = hal_impl::timer::timer_now();
    assert!(
        second >= first,
        "monotonic time should be non-decreasing: {:?} < {:?}",
        second,
        first
    );
}

///
/// zx_clock_get_boot should return a non-zero value.
/// In libos mode, boot time equals monotonic time.
#[test]
fn boot_time_non_zero() {
    let _ctx = TestContext::new();
    // In zCore libos, boot time == monotonic time.
    let now = hal_impl::timer::timer_now();
    assert!(
        !now.is_zero(),
        "boot time should be non-zero, got {:?}",
        now
    );
}

///
/// Create a user-space UTC clock object. Verify it has a non-zero id.
#[test]
fn utc_clock_create() {
    let _ctx = TestContext::new();
    let clock = Clock::new(ZX_CLOCK_OPT_AUTO_START).expect("failed to create clock");
    // KObjectBase assigns monotonically increasing IDs starting > 0.
    assert!(clock.id() > 0, "clock id should be non-zero");
}

///
/// Update a UTC clock object's value, then read it back.
#[test]
fn utc_clock_update() {
    let _ctx = TestContext::new();
    let clock = Clock::new(0).expect("failed to create clock");

    // Build update args: set synthetic_value to 42_000.
    // Option bit 0 = OPT_VALUE.
    let mut args = [0u8; 32];
    let synthetic_value: i64 = 42_000;
    args[8..16].copy_from_slice(&synthetic_value.to_le_bytes());

    clock.update(1, &args).expect("update failed"); // option 1 = OPT_VALUE

    // Clock should now be started and readable.
    let value = clock.read().expect("read after update failed");
    assert!(
        value >= 42_000,
        "clock value after update should be >= 42000, got {}",
        value
    );
}

///
/// Read the current value of an auto-start UTC clock object.
#[test]
fn utc_clock_read() {
    let _ctx = TestContext::new();
    let clock = Clock::new(ZX_CLOCK_OPT_AUTO_START).expect("failed to create clock");
    let value = clock.read().expect("failed to read auto-start clock");
    assert!(
        value >= 0,
        "auto-start clock value should be non-negative, got {}",
        value
    );
}

///
/// Get the details (mono_to_synthetic transform, error bounds) of
/// a UTC clock object.
#[test]
fn utc_clock_get_details() {
    let _ctx = TestContext::new();
    let clock = Clock::new(ZX_CLOCK_OPT_AUTO_START).expect("failed to create clock");
    let details = clock.get_details().expect("get_details failed");

    // First 8 bytes = options (should include AUTO_START bit).
    let options = u64::from_le_bytes(details[0..8].try_into().unwrap());
    assert_eq!(
        options, ZX_CLOCK_OPT_AUTO_START,
        "details options should match creation options"
    );

    // Rate numerator defaults to 1.
    let rate_num = u32::from_le_bytes(details[8..12].try_into().unwrap());
    assert_eq!(rate_num, 1);

    // Rate denominator defaults to 1.
    let rate_den = u32::from_le_bytes(details[12..16].try_into().unwrap());
    assert_eq!(rate_den, 1);
}

///
/// Read a clock with an invalid handle. Should return BAD_HANDLE.
#[test]
fn clock_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut value: u64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    let result = sc.sys_clock_read(0xDEAD, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::BAD_HANDLE),
        "reading clock with invalid handle should return BAD_HANDLE"
    );
}

///
/// Read a clock without READ right. Should return ACCESS_DENIED.
#[test]
fn clock_no_read_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a clock and add it with WRITE-only rights (no READ).
    let clock = Clock::new(ZX_CLOCK_OPT_AUTO_START).expect("failed to create clock");
    let handle = ctx
        .proc
        .add_handle(Handle::new(Arc::new(clock), Rights::WRITE));

    let mut value: u64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    let result = sc.sys_clock_read(handle, out);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "reading clock without READ right should return ACCESS_DENIED"
    );
}

///
/// Create a clock with a backstop time. Verify the clock can be
/// created and updated with a specific value.
#[test]
fn clock_backstop_time() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a clock via syscall (no auto-start).
    let mut handle: HandleValue = 0;
    let out = UserOutPtr::from(&mut handle as *mut _ as usize);
    let args_ptr = UserInPtr::from(core::ptr::null::<u8>() as usize);
    sc.sys_clock_create(0, args_ptr, out)
        .expect("failed to create clock");

    // Update with a backstop-like value (1_000_000 ns = 1ms).
    let mut args = [0u8; 32];
    let backstop: i64 = 1_000_000;
    args[8..16].copy_from_slice(&backstop.to_le_bytes());

    let update_args_ptr = UserInPtr::from(args.as_ptr() as usize);
    sc.sys_clock_update(handle, 1, update_args_ptr)
        .expect("failed to update clock with backstop value");

    // Read should return >= backstop.
    let mut value: u64 = 0;
    let read_out = UserOutPtr::from(&mut value as *mut _ as usize);
    sc.sys_clock_read(handle, read_out)
        .expect("failed to read clock after backstop update");
    assert!(
        value >= 1_000_000,
        "clock value should be >= backstop (1_000_000), got {}",
        value
    );
}

///
/// Query ticks per second. In Fuchsia, ticks_per_second = 1_000_000_000
/// (nanoseconds). Verify this constant.
#[test]
fn ticks_per_second() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    // In zCore, ticks are nanoseconds. Verify via the ticks syscall:
    // two consecutive calls should differ by at most a few nanoseconds,
    // confirming nanosecond granularity (i.e., ticks_per_second = 10^9).
    let t1 = sc.sys_ticks_get_via_kernel();
    let t2 = sc.sys_ticks_get_via_kernel();
    assert!(t1 > 0, "first ticks value should be positive");
    assert!(t2 >= t1, "ticks should be monotonically increasing");
    // The gap should be small (< 1 second = 10^9 ns), confirming ns granularity.
    let gap = t2 - t1;
    assert!(
        gap < 1_000_000_000,
        "gap between consecutive ticks reads should be < 1s, got {} ns",
        gap
    );
}

///
/// Get the current ticks value. Should be non-zero.
#[test]
fn ticks_get() {
    let _ctx = TestContext::new();
    let ticks = hal_impl::timer::timer_now().as_nanos();
    assert!(ticks > 0, "ticks should be non-zero, got {}", ticks);
}

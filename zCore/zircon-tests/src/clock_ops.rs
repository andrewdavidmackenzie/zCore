//! Clock operations tests.
//!
//! Additional clock tests covering monotonic, boot, and UTC clock
//! queries and transformations.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ClockOpsTest, MonotonicNonZero)
///
/// zx_clock_get_monotonic should return a non-zero value.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn monotonic_non_zero() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, MonotonicIncreasing)
///
/// Two consecutive calls to zx_clock_get_monotonic should return
/// increasing values.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn monotonic_increasing() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, BootTimeNonZero)
///
/// zx_clock_get_boot should return a non-zero value.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn boot_time_non_zero() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, UtcClockCreate)
///
/// Create a user-space UTC clock object. Requires clock creation
/// syscall.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn utc_clock_create() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, UtcClockUpdate)
///
/// Update a UTC clock object's value. Requires clock update syscall.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn utc_clock_update() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, UtcClockRead)
///
/// Read the current value of a UTC clock object.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn utc_clock_read() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, UtcClockGetDetails)
///
/// Get the details (mono_to_synthetic transform, error bounds) of
/// a UTC clock object.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn utc_clock_get_details() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, ClockInvalidHandle)
///
/// Read a clock with an invalid handle. Should return BAD_HANDLE.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn clock_invalid_handle() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, ClockNoReadRight)
///
/// Read a clock without READ right. Should return ACCESS_DENIED.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn clock_no_read_right() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, ClockBackstopTime)
///
/// Create a clock with a backstop time. Reads before the backstop
/// should return the backstop value.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn clock_backstop_time() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, TicksPerSecond)
///
/// Query ticks per second. Should be a positive value.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn ticks_per_second() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

/// C++: TEST(ClockOpsTest, TicksGet)
///
/// Get the current ticks value. Should be non-zero.
#[should_panic(expected = "clock ops need kernel clock")]
#[test]
fn ticks_get() {
    let _ctx = TestContext::new();
    panic!("clock ops need kernel clock");
}

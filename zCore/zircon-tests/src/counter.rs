//! Counter tests.
//!
//! Faithful 1:1 ports from CounterTest (counter.cc).
//! These test the Counter kernel object: create, read, write, add
//! with signal integration and rights enforcement.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::object::{KernelObject, Rights, Signal};
use zircon_object::signal::Counter;

extern crate alloc;

type HandleValue = u32;

/// Helper: create a counter via syscall, return handle.
fn syscall_counter_create(
    ctx: &TestContext,
    options: u32,
) -> Result<HandleValue, zircon_object::ZxError> {
    let sc = ctx.syscall();
    let mut handle: HandleValue = 0;
    let out = UserOutPtr::from(&mut handle as *mut _ as usize);
    sc.sys_counter_create(options, out)?;
    Ok(handle)
}

/// Helper: read counter value via syscall.
fn syscall_counter_read(
    ctx: &TestContext,
    handle: HandleValue,
) -> Result<i64, zircon_object::ZxError> {
    let sc = ctx.syscall();
    let mut value: i64 = 0;
    let out = UserOutPtr::from(&mut value as *mut _ as usize);
    sc.sys_counter_read(handle, out)?;
    Ok(value)
}

/// Helper: create an event via syscall, return handle.
fn syscall_event_create(ctx: &TestContext) -> HandleValue {
    let sc = ctx.syscall();
    let mut handle: HandleValue = 0;
    let out = UserOutPtr::from(&mut handle as *mut _ as usize);
    sc.sys_event_create(0, out).unwrap();
    handle
}

// ── Faithful 1:1 ports from CounterTest (counter.cc) ────────────────

/// C++: TEST(CounterTest, CreateInitial)
/// Create a counter and verify initial conditions.
#[test]
fn create_initial() {
    let ctx = TestContext::new();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Verify the value is zero.
    let value = syscall_counter_read(&ctx, handle).unwrap();
    assert_eq!(value, 0, "initial counter value should be 0");

    // Verify that only COUNTER_NON_POSITIVE is asserted.
    // We check via the kernel object directly since wait_one isn't
    // available in sync mode.
    let proc = &ctx.proc;
    let counter = proc.get_object::<Counter>(handle).unwrap();
    let sig = counter.signal();
    assert!(
        sig.contains(Signal::COUNTER_NON_POSITIVE),
        "initial signal should include COUNTER_NON_POSITIVE"
    );
    assert!(
        !sig.contains(Signal::COUNTER_POSITIVE),
        "initial signal should not include COUNTER_POSITIVE"
    );
}

/// C++: TEST(CounterTest, CreateInvalidOptions)
/// Non-zero options should fail.
#[test]
fn create_invalid_options() {
    let ctx = TestContext::new();
    assert_eq!(
        syscall_counter_create(&ctx, 1),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        syscall_counter_create(&ctx, 2),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        syscall_counter_create(&ctx, 3),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        syscall_counter_create(&ctx, u32::MAX),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
}

/// C++: TEST(CounterTest, AddInvalidHandle)
#[test]
fn add_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    // Handle 0 is invalid (not yet created).
    assert_eq!(
        sc.sys_counter_add(0xDEAD, 0),
        Err(zircon_object::ZxError::BAD_HANDLE)
    );
}

/// C++: TEST(CounterTest, AddWrongType)
#[test]
fn add_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let event_h = syscall_event_create(&ctx);
    assert_eq!(
        sc.sys_counter_add(event_h, 0),
        Err(zircon_object::ZxError::WRONG_TYPE)
    );
}

/// C++: TEST(CounterTest, AddMissingRights)
/// Add requires both READ and WRITE rights.
#[test]
fn add_missing_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Duplicate without READ.
    let mut no_read: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_read as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::READ).bits(),
        out,
    )
    .unwrap();
    assert_eq!(
        sc.sys_counter_add(no_read, 0),
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "add without READ should fail"
    );

    // Duplicate without WRITE.
    let mut no_write: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_write as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::WRITE).bits(),
        out,
    )
    .unwrap();
    assert_eq!(
        sc.sys_counter_add(no_write, 0),
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "add without WRITE should fail"
    );

    // Duplicate without both.
    let mut no_rw: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_rw as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::READ & !Rights::WRITE).bits(),
        out,
    )
    .unwrap();
    assert_eq!(
        sc.sys_counter_add(no_rw, 0),
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "add without READ|WRITE should fail"
    );
}

/// C++: TEST(CounterTest, AddOverflow)
/// Adding 1 to INT64_MAX should fail with OUT_OF_RANGE.
#[test]
fn add_overflow() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Set to INT64_MAX.
    sc.sys_counter_add(handle, i64::MAX).unwrap();

    // Verify COUNTER_POSITIVE is asserted.
    let counter = ctx.proc.get_object::<Counter>(handle).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_POSITIVE));

    // Overflow attempt.
    assert_eq!(
        sc.sys_counter_add(handle, 1),
        Err(zircon_object::ZxError::OUT_OF_RANGE),
        "overflow should fail"
    );

    // Signals unchanged after failed add.
    assert!(counter.signal().contains(Signal::COUNTER_POSITIVE));

    // Value unchanged.
    let value = syscall_counter_read(&ctx, handle).unwrap();
    assert_eq!(value, i64::MAX);
}

/// C++: TEST(CounterTest, AddUnderflow)
/// Adding -1 to INT64_MIN should fail with OUT_OF_RANGE.
#[test]
fn add_underflow() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Set to INT64_MIN.
    sc.sys_counter_add(handle, i64::MIN).unwrap();

    // Verify COUNTER_NON_POSITIVE is asserted.
    let counter = ctx.proc.get_object::<Counter>(handle).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));

    // Underflow attempt.
    assert_eq!(
        sc.sys_counter_add(handle, -1),
        Err(zircon_object::ZxError::OUT_OF_RANGE),
        "underflow should fail"
    );

    // Signals unchanged.
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));

    // Value unchanged.
    let value = syscall_counter_read(&ctx, handle).unwrap();
    assert_eq!(value, i64::MIN);
}

/// C++: TEST(CounterTest, AddHappyCase)
/// Comprehensive add test: signal transitions.
#[test]
fn add_happy_case() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();
    let counter = ctx.proc.get_object::<Counter>(handle).unwrap();

    // Initial: value=0, NON_POSITIVE.
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Add 0 to 0: signals unchanged, value unchanged.
    sc.sys_counter_add(handle, 0).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Add -1 to 0: still NON_POSITIVE, value = -1.
    sc.sys_counter_add(handle, -1).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), -1);

    // Add 1 to -1: still NON_POSITIVE, value = 0.
    sc.sys_counter_add(handle, 1).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Add 1 to 0: transitions to POSITIVE, value = 1.
    sc.sys_counter_add(handle, 1).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_POSITIVE));
    assert!(!counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 1);
}

/// C++: TEST(CounterTest, ReadInvalidHandle)
#[test]
fn read_invalid_handle() {
    let ctx = TestContext::new();
    assert_eq!(
        syscall_counter_read(&ctx, 0xDEAD),
        Err(zircon_object::ZxError::BAD_HANDLE)
    );
}

/// C++: TEST(CounterTest, ReadWrongType)
#[test]
fn read_wrong_type() {
    let ctx = TestContext::new();
    let event_h = syscall_event_create(&ctx);
    assert_eq!(
        syscall_counter_read(&ctx, event_h),
        Err(zircon_object::ZxError::WRONG_TYPE)
    );
}

/// C++: TEST(CounterTest, ReadMissingRights)
#[test]
fn read_missing_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Duplicate without READ.
    let mut no_read: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_read as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::READ).bits(),
        out,
    )
    .unwrap();
    assert_eq!(
        syscall_counter_read(&ctx, no_read),
        Err(zircon_object::ZxError::ACCESS_DENIED)
    );
}

/// C++: TEST(CounterTest, ReadHappyCase)
#[test]
fn read_happy_case() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Initial value is zero.
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Write a different value and read it back.
    sc.sys_counter_write(handle, 42).unwrap();
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 42);
}

/// C++: TEST(CounterTest, WriteInvalidHandle)
#[test]
fn write_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    assert_eq!(
        sc.sys_counter_write(0xDEAD, 0),
        Err(zircon_object::ZxError::BAD_HANDLE)
    );
}

/// C++: TEST(CounterTest, WriteWrongType)
#[test]
fn write_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let event_h = syscall_event_create(&ctx);
    assert_eq!(
        sc.sys_counter_write(event_h, 0),
        Err(zircon_object::ZxError::WRONG_TYPE)
    );
}

/// C++: TEST(CounterTest, WriteMissingRights)
#[test]
fn write_missing_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();

    // Duplicate without WRITE.
    let mut no_write: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_write as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::WRITE).bits(),
        out,
    )
    .unwrap();
    assert_eq!(
        sc.sys_counter_write(no_write, 0),
        Err(zircon_object::ZxError::ACCESS_DENIED),
        "write without WRITE should fail"
    );

    // However, no READ is OK for write.
    let mut no_read: HandleValue = 0;
    let out = UserOutPtr::from(&mut no_read as *mut _ as usize);
    sc.sys_handle_duplicate(
        handle,
        (Rights::DEFAULT_COUNTER & !Rights::READ).bits(),
        out,
    )
    .unwrap();
    assert!(
        sc.sys_counter_write(no_read, 0).is_ok(),
        "write without READ should succeed"
    );
}

/// C++: TEST(CounterTest, WriteHappyCase)
/// Comprehensive write test: signal transitions.
#[test]
fn write_happy_case() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();
    let counter = ctx.proc.get_object::<Counter>(handle).unwrap();

    // Initial: value=0, NON_POSITIVE.
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Write 0: signals unchanged, value unchanged.
    sc.sys_counter_write(handle, 0).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 0);

    // Write -1: still NON_POSITIVE.
    sc.sys_counter_write(handle, -1).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), -1);

    // Write 1: transitions to POSITIVE.
    sc.sys_counter_write(handle, 1).unwrap();
    assert!(counter.signal().contains(Signal::COUNTER_POSITIVE));
    assert!(!counter.signal().contains(Signal::COUNTER_NON_POSITIVE));
    assert_eq!(syscall_counter_read(&ctx, handle).unwrap(), 1);
}

/// C++: TEST(CounterTest, Signal)
/// User signals can be set and cleared on a counter.
#[test]
fn signal() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let handle = syscall_counter_create(&ctx, 0).unwrap();
    let counter = ctx.proc.get_object::<Counter>(handle).unwrap();

    // Signal USER_SIGNAL_0.
    sc.sys_object_signal(handle, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    let sig = counter.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::COUNTER_NON_POSITIVE));

    // Clear USER_SIGNAL_0.
    sc.sys_object_signal(handle, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    let sig = counter.signal();
    assert!(!sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::COUNTER_NON_POSITIVE));
}

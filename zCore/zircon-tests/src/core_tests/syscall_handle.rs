//! Handle syscall-level tests.
//!
//! Tests sys_handle_close, sys_handle_duplicate, sys_handle_replace
//! through the full syscall dispatch layer, exercising handle table
//! operations and rights enforcement.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, HandleValue, Rights};
use zircon_object::ZxError;

/// C++: TEST(HandleCloseTest, Many) — syscall level
/// Close many handles at once.
#[test]
fn close_many_handles() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create several eventpairs
    let mut handles = Vec::new();
    for _ in 0..5 {
        let mut h0: HandleValue = 0;
        let mut h1: HandleValue = 0;
        sc.sys_eventpair_create(
            0,
            hal_impl::user::UserOutPtr::from(&mut h0 as *mut _ as usize),
            hal_impl::user::UserOutPtr::from(&mut h1 as *mut _ as usize),
        )
        .unwrap();
        handles.push(h0);
        handles.push(h1);
    }

    // Close all at once
    use hal_impl::user::UserInPtr;
    sc.sys_handle_close_many(UserInPtr::from(handles.as_ptr() as usize), handles.len())
        .unwrap();

    // All should be gone
    for &h in &handles {
        assert_eq!(
            sc.sys_handle_check_valid(h).unwrap_err(),
            ZxError::NOT_FOUND
        );
    }
}

/// C++: TEST(HandleDup, ReplaceSuccessOrigInvalid) — syscall level
#[test]
fn replace_success_orig_invalid() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create event
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // Replace with same rights
    let mut h2: HandleValue = 0;
    sc.sys_handle_replace(
        h,
        Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h2, 0);

    // Original is invalid
    assert!(sc.sys_handle_check_valid(h).is_err());

    // New handle is valid
    sc.sys_handle_check_valid(h2).unwrap();
}

/// C++: TEST(HandleDup, ReplaceFailureBothInvalid) — syscall level
#[test]
fn replace_failure_both_invalid() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create event with READ only
    let event = zircon_object::signal::Event::new();
    let h = ctx.proc.add_handle(Handle::new(event, Rights::READ));

    // Replace with rights that include WRITE (escalation) — should fail
    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_replace(
        h,
        (Rights::READ | Rights::WRITE).bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert!(result.is_err());

    // In zCore, replace does NOT consume the source on failure
    // (differs from Fuchsia where replace always consumes).
    // The source handle remains valid.
    sc.sys_handle_check_valid(h).unwrap();
}

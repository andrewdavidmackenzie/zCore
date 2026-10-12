//! Handle syscall-level tests.
//!
//! Tests sys_handle_close, sys_handle_duplicate, sys_handle_replace
//! through the full syscall dispatch layer, exercising handle table
//! operations and rights enforcement.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, HandleValue, KernelObject, Rights};
use zircon_object::ZxError;

/// C++: TEST(HandleTest, Close) — syscall level
#[test]
fn close_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL));

    // Close via syscall
    sc.sys_handle_close(h).unwrap();

    // Handle should be gone
    assert!(ctx.proc.get_object::<Channel>(h).is_err());
}

/// C++: TEST(HandleTest, CloseInvalidHandle) — syscall level
/// Closing handle 0 (INVALID_HANDLE) is OK in Fuchsia.
#[test]
fn close_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Closing ZX_HANDLE_INVALID (0) should succeed
    sc.sys_handle_close(0).unwrap();
}

/// C++: TEST(HandleTest, Duplicate) — syscall level
#[test]
fn duplicate_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    // Include DUPLICATE right so we can duplicate the handle
    let h = ctx.proc.add_handle(Handle::new(
        ch.clone(),
        Rights::DEFAULT_CHANNEL | Rights::DUPLICATE,
    ));

    // Duplicate with same rights
    let mut h2: HandleValue = 0;
    sc.sys_handle_duplicate(
        h,
        Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h2, 0);
    assert_ne!(h, h2);

    // Both handles resolve to the same object
    let obj1 = ctx.proc.get_object::<Channel>(h).unwrap();
    let obj2 = ctx.proc.get_object::<Channel>(h2).unwrap();
    assert_eq!(obj1.id(), obj2.id());
}

/// C++: TEST(HandleTest, DuplicateWithReducedRights) — syscall level
#[test]
fn duplicate_reduced_rights() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    // Include DUPLICATE right so we can duplicate
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL | Rights::DUPLICATE));

    // Duplicate with READ only
    let mut h2: HandleValue = 0;
    sc.sys_handle_duplicate(
        h,
        Rights::READ.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h2, 0);

    // Check the new handle has reduced rights
    let (_, rights) = ctx.proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(rights, Rights::READ);
}

/// C++: TEST(HandleTest, Replace) — syscall level
#[test]
fn replace_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let ch_id = ch.id();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL));

    // Replace with READ rights
    let mut h2: HandleValue = 0;
    sc.sys_handle_replace(
        h,
        Rights::READ.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h2, 0);

    // Original handle is gone
    assert!(ctx.proc.get_object::<Channel>(h).is_err());

    // New handle works and has reduced rights
    let (obj, rights) = ctx.proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(obj.id(), ch_id);
    assert_eq!(rights, Rights::READ);
}

/// C++: TEST(HandleTest, DuplicateExpandedRightsFails) — syscall level
/// Cannot escalate rights via duplicate.
#[test]
fn duplicate_expanded_rights_fails() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    // Add with READ only
    let h = ctx.proc.add_handle(Handle::new(ch, Rights::READ));

    // Try to duplicate with READ | WRITE — should fail
    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_duplicate(
        h,
        (Rights::READ | Rights::WRITE).bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(HandleTest, DuplicateRequiresDuplicateRight) — syscall level
/// Handle must have DUPLICATE right to be duplicated.
#[test]
fn duplicate_requires_duplicate_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    // Add without DUPLICATE right
    let h = ctx.proc.add_handle(Handle::new(
        ch,
        Rights::READ | Rights::WRITE, // no DUPLICATE
    ));

    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_duplicate(
        h,
        Rights::READ.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

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

/// C++: TEST(HandleTest, CloseBadHandle) — syscall level
/// Closing a non-existent (but non-zero) handle fails.
#[test]
fn close_bad_handle_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // A non-zero, non-existent handle should fail
    let result = sc.sys_handle_close(0xDEAD_BEEF);
    assert!(result.is_err());
}

/// C++: TEST(HandleTest, DuplicateInvalidHandle) — syscall level
/// Duplicating a non-existent handle fails.
#[test]
fn duplicate_invalid_handle_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_duplicate(
        0xDEAD_BEEF,
        Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert!(result.is_err());
    assert_eq!(h2, 0, "output handle should not be written on failure");
}

/// C++: TEST(HandleTest, DuplicateSameRights) — syscall level
/// Duplicating with SAME_RIGHTS preserves the original rights.
#[test]
fn duplicate_same_rights_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let original_rights = Rights::READ | Rights::WRITE | Rights::DUPLICATE;
    let h = ctx.proc.add_handle(Handle::new(ch, original_rights));

    let mut h2: HandleValue = 0;
    sc.sys_handle_duplicate(
        h,
        Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h2, 0);

    // New handle should have the same rights as the original
    let (_, rights) = ctx.proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(rights, original_rights);
}

/// C++: TEST(HandleTest, ReplaceBadHandle) — syscall level
/// Replacing a non-existent handle fails.
#[test]
fn replace_bad_handle_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_replace(
        0xDEAD_BEEF,
        Rights::READ.bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert!(result.is_err());
}

/// C++: TEST(HandleTest, ReplaceInvalidRights) — syscall level
/// Replace with rights that exceed the original handle's rights fails.
#[test]
fn replace_invalid_rights_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    // Only READ right
    let h = ctx.proc.add_handle(Handle::new(ch, Rights::READ));

    let mut h2: HandleValue = 0;
    let result = sc.sys_handle_replace(
        h,
        (Rights::READ | Rights::WRITE).bits(),
        UserOutPtr::from(&mut h2 as *mut _ as usize),
    );
    assert!(result.is_err());
}

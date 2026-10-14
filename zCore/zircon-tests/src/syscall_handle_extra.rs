//! Additional zCore-specific tests for syscall_handle functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::ZxError;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, HandleValue, KernelObject, Rights};

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

/// Closing handle 0 (INVALID_HANDLE) is OK in Fuchsia.
#[test]
fn close_invalid_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Closing ZX_HANDLE_INVALID (0) should succeed
    sc.sys_handle_close(0).unwrap();
}

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

/// Closing a non-existent (but non-zero) handle fails.
#[test]
fn close_bad_handle_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // A non-zero, non-existent handle should fail
    let result = sc.sys_handle_close(0xDEAD_BEEF);
    assert!(result.is_err());
}

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


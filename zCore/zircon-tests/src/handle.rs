//! Handle syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::ZxError;

#[test]
fn handle_close() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    assert!(proc.get_object::<Channel>(h).is_ok());
    proc.remove_handle(h).unwrap();
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

#[test]
fn handle_duplicate() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::DEFAULT_CHANNEL))
        .unwrap();
    assert_ne!(h, h2);

    // Both handles should reference the same object
    let obj1 = proc.get_object::<Channel>(h).unwrap();
    let obj2 = proc.get_object::<Channel>(h2).unwrap();
    assert_eq!(obj1.id(), obj2.id());
}

#[test]
fn handle_replace() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0.clone(), Rights::DEFAULT_CHANNEL));

    // Replace with reduced rights
    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::READ))
        .unwrap();
    proc.remove_handle(h).unwrap();

    // Original handle is gone
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );
    // New handle works and references the same object with reduced rights
    let (obj, rights) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(obj.id(), ch0.id());
    assert_eq!(rights, Rights::READ);
}

#[test]
fn handle_invalid() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    // Handle 0 is always invalid
    assert_eq!(
        proc.get_object::<Channel>(0).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Random handle value is invalid
    assert_eq!(
        proc.get_object::<Channel>(0xDEAD_BEEF).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleTest, Close)
/// Closing a handle makes it invalid.
#[test]
fn handle_close_cxx() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    assert!(proc.get_object::<Channel>(h).is_ok());

    proc.remove_handle(h).unwrap();
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleTest, CloseMany)
/// Closing multiple handles at once.
#[test]
fn handle_close_many() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let (ch2, _ch3) = Channel::create();
    let (ch4, _ch5) = Channel::create();

    let h1 = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    let h2 = proc.add_handle(Handle::new(ch2, Rights::DEFAULT_CHANNEL));
    let h3 = proc.add_handle(Handle::new(ch4, Rights::DEFAULT_CHANNEL));

    // Remove all handles
    proc.remove_handles(&[h1, h2, h3]).unwrap();

    // All should be gone
    assert_eq!(
        proc.get_object::<Channel>(h1).unwrap_err(),
        ZxError::BAD_HANDLE
    );
    assert_eq!(
        proc.get_object::<Channel>(h2).unwrap_err(),
        ZxError::BAD_HANDLE
    );
    assert_eq!(
        proc.get_object::<Channel>(h3).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleTest, CloseBadHandle)
/// Closing an invalid handle returns BAD_HANDLE.
#[test]
fn handle_close_bad_handle() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    // Closing a handle that doesn't exist should fail
    assert_eq!(
        proc.remove_handle(0xDEAD_BEEF).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleTest, DuplicateInvalidHandle)
/// Duplicating an invalid handle fails.
#[test]
fn handle_duplicate_invalid() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let result = proc.dup_handle_operating_rights(0xDEAD_BEEF, |r| Ok(r));
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(HandleTest, DuplicateSameRights)
/// Duplicating a handle with the same rights preserves them.
#[test]
fn handle_duplicate_same_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let original_rights = Rights::READ | Rights::WRITE | Rights::DUPLICATE;
    let h = proc.add_handle(Handle::new(ch0, original_rights));

    // Duplicate with same rights
    let h2 = proc.dup_handle_operating_rights(h, |r| Ok(r)).unwrap();
    assert_ne!(h, h2);

    // Check rights are preserved
    let (_, rights) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(rights, original_rights);
}

/// C++: TEST(HandleTest, DuplicateReducedRights)
/// Duplicating a handle with a subset of rights succeeds.
#[test]
fn handle_duplicate_reduced_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(
        ch0,
        Rights::READ | Rights::WRITE | Rights::DUPLICATE,
    ));

    // Duplicate with READ only
    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::READ))
        .unwrap();

    let (_, rights) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(rights, Rights::READ);
}

/// C++: TEST(HandleTest, DuplicateExpandedRightsFails)
/// Duplicating with rights not in the original fails (at the object level).
#[test]
fn handle_duplicate_expanded_rights_fails() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    // Only READ right
    let h = proc.add_handle(Handle::new(ch0, Rights::READ));

    // Try to duplicate with READ | WRITE — the closure should reject
    let result = proc.dup_handle_operating_rights(h, |handle_rights| {
        let desired = Rights::READ | Rights::WRITE;
        if (handle_rights & desired).bits() != desired.bits() {
            Err(ZxError::ACCESS_DENIED)
        } else {
            Ok(desired)
        }
    });
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(HandleTest, Replace)
/// Replace a handle with new rights.
#[test]
fn handle_replace_cxx() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let ch0_id = ch0.id();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Duplicate with reduced rights, then remove original (= replace)
    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::READ))
        .unwrap();
    proc.remove_handle(h).unwrap();

    // Original is gone
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // New handle works with reduced rights
    let (obj, rights) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(obj.id(), ch0_id);
    assert_eq!(rights, Rights::READ);
}

/// C++: TEST(HandleTest, ReplaceBadHandle)
/// Replacing an invalid handle fails.
#[test]
fn handle_replace_bad_handle() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let result = proc.dup_handle_operating_rights(0xDEAD_BEEF, |_| Ok(Rights::READ));
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(HandleTest, ReplaceInvalidRights)
/// Replacing with rights that exceed the original fails.
#[test]
fn handle_replace_invalid_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    // Only READ right
    let h = proc.add_handle(Handle::new(ch0, Rights::READ));

    // Attempt to "replace" with WRITE right (not in original)
    let result = proc.dup_handle_operating_rights(h, |handle_rights| {
        let desired = Rights::WRITE;
        if (handle_rights & desired).bits() != desired.bits() {
            Err(ZxError::INVALID_ARGS)
        } else {
            Ok(desired)
        }
    });
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

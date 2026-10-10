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
    // New handle works
    assert!(proc.get_object::<Channel>(h2).is_ok());
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

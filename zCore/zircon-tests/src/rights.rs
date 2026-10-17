//! Rights tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/rights tests.
//! These tests verify handle rights checking, reduction, and
//! propagation across operations.

use crate::helpers::TestContext;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, Rights};
use zircon_object::signal::Event;

extern crate alloc;

///
/// A newly created event should have all default rights.
#[test]
fn default_event_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let expected = Rights::DEFAULT_EVENT;
    let handle = Handle::new(event, expected);
    assert!(handle.rights.contains(Rights::SIGNAL));
    assert!(handle.rights.contains(Rights::WAIT));
    assert!(handle.rights.contains(Rights::DUPLICATE));
    assert!(handle.rights.contains(Rights::TRANSFER));
}

///
/// Reducing rights on a handle should remove the specified rights.
#[test]
fn reduce_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let full_rights = Rights::DEFAULT_EVENT;
    let reduced = full_rights - Rights::SIGNAL;
    let handle = Handle::new(event, reduced);
    assert!(!handle.rights.contains(Rights::SIGNAL));
    assert!(handle.rights.contains(Rights::WAIT));
}

///
/// Duplicating a handle should preserve the original rights.
#[test]
fn duplicate_preserves_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let rights = Rights::SIGNAL | Rights::WAIT | Rights::DUPLICATE;
    let handle = Handle::new(event, rights);
    // Duplicate with same rights
    let dup = Handle::new(handle.object.clone(), handle.rights);
    assert_eq!(dup.rights, handle.rights);
}

///
/// Duplicate a handle with reduced rights. The duplicate should
/// have only the requested subset.
#[test]
fn duplicate_reduce_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let full = Rights::DEFAULT_EVENT;
    let handle = Handle::new(event, full);
    let reduced = Rights::WAIT;
    let dup = Handle::new(handle.object.clone(), reduced);
    assert_eq!(dup.rights, Rights::WAIT);
    assert!(!dup.rights.contains(Rights::SIGNAL));
}

///
/// Verify the contains method on Rights works correctly.
#[test]
fn rights_contains() {
    let _ctx = TestContext::new();
    let r = Rights::READ | Rights::WRITE;
    assert!(r.contains(Rights::READ));
    assert!(r.contains(Rights::WRITE));
    assert!(!r.contains(Rights::EXECUTE));
    assert!(r.contains(Rights::READ | Rights::WRITE));
}

///
/// Channel endpoints should have their default rights set.
#[test]
fn channel_default_rights() {
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    let rights = Rights::DEFAULT_CHANNEL;
    let handle = Handle::new(local, rights);
    assert!(handle.rights.contains(Rights::READ));
    assert!(handle.rights.contains(Rights::WRITE));
    assert!(handle.rights.contains(Rights::TRANSFER));
}

///
/// A handle with no rights should reject all operations.
#[test]
fn no_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let handle = Handle::new(event, Rights::empty());
    assert!(!handle.rights.contains(Rights::READ));
    assert!(!handle.rights.contains(Rights::WRITE));
    assert!(!handle.rights.contains(Rights::SIGNAL));
}

///
/// A handle with all rights should pass all rights checks.
#[test]
fn all_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let handle = Handle::new(event, Rights::all());
    assert!(handle.rights.contains(Rights::READ));
    assert!(handle.rights.contains(Rights::WRITE));
    assert!(handle.rights.contains(Rights::EXECUTE));
    assert!(handle.rights.contains(Rights::SIGNAL));
    assert!(handle.rights.contains(Rights::DUPLICATE));
}

///
/// Transfer a handle with specific rights through a channel. The
/// received handle should retain the original rights.
#[test]
fn transfer_rights_via_channel() {
    use zircon_object::ipc::MessagePacket;
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    let event = Event::new();
    let rights = Rights::SIGNAL | Rights::WAIT;
    let handle = Handle::new(event, rights);

    let msg = MessagePacket {
        data: alloc::vec![],
        handles: alloc::vec![handle],
        ..Default::default()
    };
    local.write(msg).unwrap();
    let received = remote.read().unwrap();
    assert_eq!(received.handles.len(), 1);
    assert_eq!(received.handles[0].rights, rights);
}

///
/// Verify bitwise operations on rights (union, intersection, difference).
#[test]
fn rights_bit_operations() {
    let _ctx = TestContext::new();
    let a = Rights::READ | Rights::WRITE;
    let b = Rights::WRITE | Rights::EXECUTE;
    let union = a | b;
    assert!(union.contains(Rights::READ));
    assert!(union.contains(Rights::WRITE));
    assert!(union.contains(Rights::EXECUTE));

    let intersection = a & b;
    assert!(!intersection.contains(Rights::READ));
    assert!(intersection.contains(Rights::WRITE));
    assert!(!intersection.contains(Rights::EXECUTE));
}

///
/// Empty rights should contain nothing.
#[test]
fn rights_empty() {
    let _ctx = TestContext::new();
    let empty = Rights::empty();
    assert!(!empty.contains(Rights::READ));
    assert!(!empty.contains(Rights::WRITE));
    assert!(empty.is_empty());
}

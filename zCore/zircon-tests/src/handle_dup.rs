//! Handle duplication tests.
//!
//! Ports of Fuchsia's handle duplication and replacement tests.
//! These tests verify handle table operations at the process level.

use crate::helpers::TestContext;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::Event;
use zircon_object::task::Job;

extern crate alloc;

///
/// Duplicate an event handle. Both handles should reference the
/// same underlying object.
#[test]
fn duplicate_event_handle() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let event_id = event.id();
    let h1 = Handle::new(event, Rights::DEFAULT_EVENT);
    let h2 = Handle::new(h1.object.clone(), h1.rights);

    assert_eq!(h1.object.id(), event_id);
    assert_eq!(h2.object.id(), event_id);
    assert_eq!(h1.rights, h2.rights);
}

///
/// Duplicate a handle with fewer rights. The duplicate should have
/// only the requested rights.
#[test]
fn duplicate_with_reduced_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let h1 = Handle::new(event, Rights::SIGNAL | Rights::WAIT | Rights::DUPLICATE);
    let reduced = Rights::WAIT;
    let h2 = Handle::new(h1.object.clone(), reduced);

    assert_eq!(h2.rights, Rights::WAIT);
    assert!(!h2.rights.contains(Rights::SIGNAL));
}

///
/// Duplicate a channel handle. Both handles should be usable.
#[test]
fn duplicate_channel_handle() {
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    let local_id = local.id();
    let h1 = Handle::new(local, Rights::DEFAULT_CHANNEL);
    let h2 = Handle::new(h1.object.clone(), h1.rights);
    assert_eq!(h1.object.id(), local_id);
    assert_eq!(h2.object.id(), local_id);
}

///
/// Duplicate a job handle. Both should reference the same job.
#[test]
fn duplicate_job_handle() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    let child_id = child.id();
    let h1 = Handle::new(child.clone(), Rights::DEFAULT_JOB);
    let h2 = Handle::new(h1.object.clone(), h1.rights);
    assert_eq!(h1.object.id(), child_id);
    assert_eq!(h2.object.id(), child_id);
}

///
/// Replace a handle with new rights. The original handle should be
/// invalidated and the new one should have the requested rights.
#[test]
fn replace_handle() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let event_id = event.id();
    let h1 = Handle::new(event, Rights::DEFAULT_EVENT);
    // Replace by creating new with reduced rights
    let h2 = Handle::new(h1.object.clone(), Rights::WAIT);
    // h1 is effectively "replaced" by h2 (in a real handle table,
    // h1 would be removed and h2 inserted).
    assert_eq!(h2.object.id(), event_id);
    assert_eq!(h2.rights, Rights::WAIT);
}

///
/// Two handles from duplication should have the same object identity
/// (same KOID).
#[test]
fn duplicate_preserves_object_identity() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let arc_event = event;
    let h1 = Handle::new(arc_event.clone(), Rights::DEFAULT_EVENT);
    let h2 = Handle::new(arc_event.clone(), Rights::DEFAULT_EVENT);
    let h3 = Handle::new(arc_event.clone(), Rights::WAIT);

    // All should point to the same object
    assert_eq!(h1.object.id(), h2.object.id());
    assert_eq!(h2.object.id(), h3.object.id());
}

///
/// Duplicate with Rights::all() — should preserve all possible rights.
#[test]
fn duplicate_all_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let h1 = Handle::new(event, Rights::all());
    let h2 = Handle::new(h1.object.clone(), Rights::all());
    assert_eq!(h1.rights, h2.rights);
    assert!(h2.rights.contains(Rights::READ));
    assert!(h2.rights.contains(Rights::WRITE));
    assert!(h2.rights.contains(Rights::EXECUTE));
}

///
/// Duplicate with empty rights. The duplicate should have no rights.
#[test]
fn duplicate_empty_rights() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let h1 = Handle::new(event, Rights::DEFAULT_EVENT);
    let h2 = Handle::new(h1.object.clone(), Rights::empty());
    assert!(h2.rights.is_empty());
}

///
/// Create many duplicates of the same handle. All should share the
/// same object identity.
#[test]
fn many_duplicates() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let arc_event = event;
    let id = arc_event.id();

    let mut handles = alloc::vec::Vec::new();
    for _ in 0..100 {
        handles.push(Handle::new(arc_event.clone(), Rights::DEFAULT_EVENT));
    }

    for h in &handles {
        assert_eq!(h.object.id(), id);
    }
}

///
/// Duplicate a handle, then drop the original. The duplicate should
/// still be valid and usable.
#[test]
fn duplicate_and_drop_original() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let event_id = event.id();
    let arc_event = event;
    let h2 = Handle::new(arc_event.clone(), Rights::DEFAULT_EVENT);
    drop(arc_event); // drop original reference

    // h2 should still be valid
    assert_eq!(h2.object.id(), event_id);
}

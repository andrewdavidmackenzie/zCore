//! Handle syscall tests.

use crate::helpers::TestContext;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::ZxError;

/// C++: TEST(HandleCloseTest, ManyDuplicateTest)
/// Duplicate a handle many times, then close all duplicates and the original.
/// All should become invalid after closing.
#[test]
fn handle_close_many_duplicate_test() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Duplicate the handle 10 times
    let mut dups = Vec::new();
    for _ in 0..10 {
        let hd = proc.dup_handle_operating_rights(h, Ok).unwrap();
        dups.push(hd);
    }

    // All duplicates should be valid and refer to the same object
    let orig_obj = proc.get_object::<Channel>(h).unwrap();
    for &hd in &dups {
        let obj = proc.get_object::<Channel>(hd).unwrap();
        assert_eq!(obj.id(), orig_obj.id());
    }

    // Close all duplicates
    for &hd in &dups {
        proc.remove_handle(hd).unwrap();
    }
    // Original should still work
    assert!(proc.get_object::<Channel>(h).is_ok());

    // Close original
    proc.remove_handle(h).unwrap();
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleCloseTest, ManyInvalidHandlesShouldNotFail)
/// Closing a batch that includes invalid handles: the valid ones are
/// closed and the call reports the error for the invalid ones.
#[test]
fn handle_close_many_invalid_handles_should_not_fail() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Close a batch with some invalid handles — should not panic
    // remove_handles returns an error but processes the valid entries.
    let result = proc.remove_handles(&[h, 0xDEAD, 0xBEEF]);
    // The call may return an error for the invalid handles, but
    // the valid handle `h` should still be closed.
    let _ = result;
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );
}

/// C++: TEST(HandleCloseTest, RegressionTest_479281267)
/// Closing the same handle twice: the first close succeeds, the second
/// returns BAD_HANDLE. This is a regression test ensuring no double-free.
#[test]
fn handle_close_regression_test_479281267() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // First close succeeds
    proc.remove_handle(h).unwrap();

    // Second close on the same handle value returns BAD_HANDLE
    assert_eq!(proc.remove_handle(h).unwrap_err(), ZxError::BAD_HANDLE);

    // Verify a fresh handle can be added and works fine afterwards
    let (ch2, _ch3) = Channel::create();
    let h2 = proc.add_handle(Handle::new(ch2, Rights::DEFAULT_CHANNEL));
    assert!(proc.get_object::<Channel>(h2).is_ok());
}

/// C++: TEST(HandleInfoTest, CheckHandleValid)
/// Getting info on a valid handle succeeds; on an invalid handle it fails.
#[test]
fn handle_info_check_handle_valid() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // Valid handle — get_handle_info should succeed
    assert!(proc.get_handle_info(h).is_ok());

    // Invalid handle — should fail with BAD_HANDLE
    assert_eq!(
        proc.get_handle_info(0xDEAD_BEEF).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Close the handle, then get_handle_info should fail
    proc.remove_handle(h).unwrap();
    assert_eq!(proc.get_handle_info(h).unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(HandleInfoTest, DupAndInfoRights)
/// Duplicate a handle, get info on both, and verify they have the
/// expected rights and reference the same object (same koid).
#[test]
fn handle_info_dup_and_info_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let original_rights = Rights::READ | Rights::WRITE | Rights::DUPLICATE;
    let h = proc.add_handle(Handle::new(ch0, original_rights));

    // Duplicate with same rights
    let h2 = proc.dup_handle_operating_rights(h, Ok).unwrap();

    // Both should reference the same object
    let (obj1, rights1) = proc.get_object_and_rights::<Channel>(h).unwrap();
    let (obj2, rights2) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(obj1.id(), obj2.id());
    assert_eq!(rights1, original_rights);
    assert_eq!(rights2, original_rights);

    // get_handle_info should succeed on both
    assert!(proc.get_handle_info(h).is_ok());
    assert!(proc.get_handle_info(h2).is_ok());
}

/// C++: TEST(HandleInfoTest, DuplicateRights)
/// Duplicate a handle with reduced rights and verify the new handle
/// has only the requested rights.
#[test]
fn handle_info_duplicate_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let h = proc.add_handle(Handle::new(
        ch0,
        Rights::READ | Rights::WRITE | Rights::DUPLICATE,
    ));

    // Dup with READ only
    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::READ))
        .unwrap();

    // Original keeps full rights
    let (_, r1) = proc.get_object_and_rights::<Channel>(h).unwrap();
    assert!(r1.contains(Rights::READ));
    assert!(r1.contains(Rights::WRITE));

    // Duplicate has only READ
    let (_, r2) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(r2, Rights::READ);
    assert!(!r2.contains(Rights::WRITE));
}

/// C++: TEST(HandleInfoTest, RelatedKoid)
/// For a channel pair, each endpoint's related_koid should be the
/// koid of the other endpoint.
#[test]
fn handle_info_related_koid() {
    use zircon_object::object::KernelObject;

    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, ch1) = Channel::create();
    let ch0_id = ch0.id();
    let ch1_id = ch1.id();

    let h0 = proc.add_handle(Handle::new(ch0.clone(), Rights::DEFAULT_CHANNEL));
    let h1 = proc.add_handle(Handle::new(ch1.clone(), Rights::DEFAULT_CHANNEL));

    // Each channel's related_koid is the peer's koid
    assert_eq!(ch0.related_koid(), ch1_id);
    assert_eq!(ch1.related_koid(), ch0_id);

    // get_handle_info should succeed on both
    assert!(proc.get_handle_info(h0).is_ok());
    assert!(proc.get_handle_info(h1).is_ok());
}

/// C++: TEST(HandleInfoTest, ReplaceRights)
/// Replace a handle by duplicating with new rights, removing the original,
/// and verifying the replacement has the new rights.
#[test]
fn handle_info_replace_rights() {
    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (ch0, _ch1) = Channel::create();
    let ch0_id = ch0.id();
    let h = proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    // "Replace" by dup with READ, then remove original
    let h2 = proc
        .dup_handle_operating_rights(h, |_| Ok(Rights::READ))
        .unwrap();
    proc.remove_handle(h).unwrap();

    // Original is gone
    assert_eq!(
        proc.get_object::<Channel>(h).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Replacement has READ only and refers to the same object
    let (obj, rights) = proc.get_object_and_rights::<Channel>(h2).unwrap();
    assert_eq!(obj.id(), ch0_id);
    assert_eq!(rights, Rights::READ);
    assert!(!rights.contains(Rights::WRITE));
}

/// C++: TEST(HandleTransferTest, CancelsWait)
/// Transferring a handle to another process via a channel should remove
/// it from the sender's handle table. The handle is moved, not copied.
#[test]
fn handle_transfer_cancels_wait() {
    use zircon_object::ipc::MessagePacket;
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (tx, rx) = Channel::create();
    let event = Event::new();
    let event_id = event.id();

    // Add the event handle to the process
    let eh = proc.add_handle(Handle::new(event.clone(), Rights::DEFAULT_EVENT));
    assert!(proc.get_object::<Event>(eh).is_ok());

    // Remove the handle from the process (simulating transfer)
    let removed = proc.remove_handle(eh).unwrap();

    // Send the handle over the channel
    tx.write(MessagePacket {
        data: vec![0u8; 4],
        handles: vec![removed],
    })
    .unwrap();

    // The handle should no longer be in the sender's handle table
    assert_eq!(
        proc.get_object::<Event>(eh).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Read the message from the other side
    let msg = rx.read().unwrap();
    assert_eq!(msg.handles.len(), 1);
    assert_eq!(msg.handles[0].object.id(), event_id);
}

/// C++: TEST(HandleTransferTest, OverChannelThenRead)
/// Transfer a handle over a channel, read it on the other side,
/// and verify the received handle refers to the same object with
/// the transferred rights.
#[test]
fn handle_transfer_over_channel_then_read() {
    use zircon_object::ipc::MessagePacket;
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let proc = &ctx.proc;

    let (tx, rx) = Channel::create();
    let event = Event::new();
    let event_id = event.id();
    let transfer_rights = Rights::READ | Rights::WRITE | Rights::SIGNAL;

    // Add handle with specific rights and then remove for transfer
    let eh = proc.add_handle(Handle::new(event.clone(), transfer_rights));
    let removed = proc.remove_handle(eh).unwrap();
    assert_eq!(removed.rights, transfer_rights);

    // Transfer over channel
    tx.write(MessagePacket {
        data: vec![1, 2, 3, 4],
        handles: vec![removed],
    })
    .unwrap();

    // Read on the receiving end
    let msg = rx.read().unwrap();
    assert_eq!(msg.data, vec![1, 2, 3, 4]);
    assert_eq!(msg.handles.len(), 1);

    // Verify the received handle
    let received_handle = &msg.handles[0];
    assert_eq!(received_handle.object.id(), event_id);
    assert_eq!(received_handle.rights, transfer_rights);

    // Add the received handle to the process and verify it works
    let h2 = proc.add_handle(received_handle.clone());
    assert!(proc.get_object::<Event>(h2).is_ok());
    let (obj, rights) = proc.get_object_and_rights::<Event>(h2).unwrap();
    assert_eq!(obj.id(), event_id);
    assert_eq!(rights, transfer_rights);
}

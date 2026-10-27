//! EventPair tests.
//!
//! Faithful 1:1 ports of Fuchsia's `zircon/system/utest/core/event-pair/event-pair.cc`.

use crate::helpers::TestContext;
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::signal::EventPair;

use super::object_info::{get_info, HANDLE_BASIC_SIZE, TOPIC_HANDLE_BASIC};

/// C++: TEST(EventPairTest, HandlesNotInvalid)
#[test]
fn handles_not_invalid() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    assert_ne!(ep0.id(), 0);
    assert_ne!(ep1.id(), 0);
}

/// C++: TEST(EventPairTest, HandleRightsAreCorrect)
///
/// Verify handle type is EVENTPAIR (16) and rights are BASIC | SIGNAL | SIGNAL_PEER.
#[test]
fn handle_rights_are_correct() {
    let ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    let expected_rights = Rights::DEFAULT_EVENTPAIR;
    let h0 = ctx.proc.add_handle(Handle::new(ep0, expected_rights));
    let h1 = ctx.proc.add_handle(Handle::new(ep1, expected_rights));

    // HandleBasicInfo layout: koid(u64,0) rights(u32,8) obj_type(u32,12)
    //                         related_koid(u64,16) props(u32,24) padding(u32,28)

    // Check ep0 via ZX_INFO_HANDLE_BASIC.
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, h0, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
    // obj_type at offset 12 (u32): EventPair = 16
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    assert_eq!(obj_type, 16, "expected EventPair type (16)");
    // rights at offset 8 (u32)
    let rights = u32::from_le_bytes(buf[8..12].try_into().unwrap());
    assert_eq!(
        rights,
        expected_rights.bits(),
        "expected DEFAULT_EVENTPAIR rights"
    );

    // Check ep1 via ZX_INFO_HANDLE_BASIC.
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, h1, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    assert_eq!(obj_type, 16, "expected EventPair type (16)");
    let rights = u32::from_le_bytes(buf[8..12].try_into().unwrap());
    assert_eq!(
        rights,
        expected_rights.bits(),
        "expected DEFAULT_EVENTPAIR rights"
    );
}

/// C++: TEST(EventPairTest, KoidsAreCorrect)
///
/// Verify koid and related_koid via ZX_INFO_HANDLE_BASIC for both endpoints.
#[test]
fn koids_are_correct() {
    let ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    let koid0 = ep0.id();
    let koid1 = ep1.id();

    assert_ne!(koid0, 0);
    assert_ne!(koid1, 0);

    let h0 = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));
    let h1 = ctx
        .proc
        .add_handle(Handle::new(ep1, Rights::DEFAULT_EVENTPAIR));

    // HandleBasicInfo layout: koid(u64,0) rights(u32,8) obj_type(u32,12)
    //                         related_koid(u64,16) props(u32,24) padding(u32,28)

    // Check ep0 via ZX_INFO_HANDLE_BASIC.
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    get_info(&ctx, h0, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    let info_koid0 = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let info_related0 = u64::from_le_bytes(buf[16..24].try_into().unwrap());
    assert_eq!(info_koid0, koid0);
    assert_ne!(info_related0, 0);
    assert_eq!(info_related0, koid1, "ep0.related_koid should be ep1.koid");

    // Check ep1 via ZX_INFO_HANDLE_BASIC.
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    get_info(&ctx, h1, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    let info_koid1 = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let info_related1 = u64::from_le_bytes(buf[16..24].try_into().unwrap());
    assert_eq!(info_koid1, koid1);
    assert_ne!(info_related1, 0);
    assert_eq!(info_related1, koid0, "ep1.related_koid should be ep0.koid");
}

/// C++: TEST(EventPairTest, CheckNoFlagsSupported)
///
/// Currently no flags are supported for eventpair creation.
/// In C++, `create(1, ...)` returns ZX_ERR_NOT_SUPPORTED.
/// In zCore, EventPair::create() takes no flags, so this tests
/// the syscall layer rejection via sys_eventpair_create.
#[test]
fn check_no_flags_supported() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let out0 = UserOutPtr::from(&mut h0 as *mut _ as usize);
    let out1 = UserOutPtr::from(&mut h1 as *mut _ as usize);

    // options=1 should fail
    let result = sc.sys_eventpair_create(1, out0, out1);
    assert_eq!(result, Err(zircon_object::ZxError::NOT_SUPPORTED));
    // handles should remain invalid (0)
    assert_eq!(h0, 0);
    assert_eq!(h1, 0);
}

/// C++: TEST(EventPairTest, SignalEventPairAndClearVerifySignals)
///
/// Set a user signal on self, verify, then clear it and verify again.
#[test]
fn signal_event_pair_and_clear_verify_signals() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    // Initially no signals on either endpoint.
    assert_eq!(ep0.signal(), Signal::empty());
    assert_eq!(ep1.signal(), Signal::empty());

    // Set USER_SIGNAL_0 on ep0.
    ep0.signal_set(Signal::USER_SIGNAL_0);
    assert_eq!(ep0.signal(), Signal::USER_SIGNAL_0);
    assert_eq!(ep1.signal(), Signal::empty());

    // Clear USER_SIGNAL_0 on ep0.
    ep0.signal_clear(Signal::USER_SIGNAL_0);
    assert_eq!(ep0.signal(), Signal::empty());
    assert_eq!(ep1.signal(), Signal::empty());
}

/// C++: TEST(EventPairTest, SignalPeerAndVerifyRecived)
///
/// Signal the peer endpoint with various user signals and verify
/// they appear on the correct side.
#[test]
fn signal_peer_and_verify_received() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    // ep0 signals peer (ep1) with USER_SIGNAL_0.
    ep0.peer().unwrap().signal_set(Signal::USER_SIGNAL_0);
    assert_eq!(ep0.signal(), Signal::empty());
    assert_eq!(ep1.signal(), Signal::USER_SIGNAL_0);

    // ep1 signals peer (ep0) with USER_SIGNAL_1 | USER_SIGNAL_2.
    ep1.peer()
        .unwrap()
        .signal_set(Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    assert_eq!(ep0.signal(), Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    assert_eq!(ep1.signal(), Signal::USER_SIGNAL_0);

    // ep0 clears USER_SIGNAL_0 on peer (ep1) and sets USER_SIGNAL_3 | USER_SIGNAL_4.
    ep0.peer().unwrap().signal_clear(Signal::USER_SIGNAL_0);
    ep0.peer()
        .unwrap()
        .signal_set(Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4);
    assert_eq!(ep0.signal(), Signal::USER_SIGNAL_1 | Signal::USER_SIGNAL_2);
    assert_eq!(ep1.signal(), Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4);
}

/// C++: TEST(EventPairTest, SignalPeerThenCloseAndVerifySignalReceived)
///
/// Signal peer, close original, verify signals + PEER_CLOSED on survivor.
#[test]
fn signal_peer_then_close_and_verify_signal_received() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    ep0.peer()
        .unwrap()
        .signal_set(Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4);

    // Drop ep0.
    drop(ep0);

    // ep1 should see PEER_CLOSED + the previously set signals.
    assert_eq!(
        ep1.signal(),
        Signal::PEER_CLOSED | Signal::USER_SIGNAL_3 | Signal::USER_SIGNAL_4
    );
}

/// C++: TEST(EventPairTest, SignalingClosedPeerReturnsPeerClosed)
///
/// After peer is closed, signal_peer should return ZX_ERR_PEER_CLOSED.
#[test]
fn signaling_closed_peer_returns_peer_closed() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, ep1) = EventPair::create();
    let h0 = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    // Close the peer.
    drop(ep1);

    // sys_object_signal_peer should return PEER_CLOSED.
    let result = sc.sys_object_signal_peer(h0, 0, Signal::USER_SIGNAL_0.bits());
    assert_eq!(result, Err(zircon_object::ZxError::PEER_CLOSED));
}

/// C++: TEST(EventPairTest, SignalAccessDeniedMissingSignalRight)
///
/// Duplicate handle without SIGNAL right, verify signal returns ACCESS_DENIED.
/// At the object level, rights are not enforced — this tests the concept
/// by verifying signal_change works with appropriate rights at syscall level.
#[test]
fn signal_access_denied_missing_signal_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, _ep1) = EventPair::create();
    // Add handle without SIGNAL right.
    let rights = Rights::DEFAULT_EVENTPAIR & !Rights::SIGNAL;
    let h = ctx.proc.add_handle(Handle::new(ep0, rights));

    // sys_object_signal should fail with ACCESS_DENIED.
    let result = sc.sys_object_signal(h, 0, Signal::USER_SIGNAL_0.bits());
    assert_eq!(result, Err(zircon_object::ZxError::ACCESS_DENIED));
}

/// C++: TEST(EventPairTest, SignalPeerAccessDeniedMissingSignalPeerRight)
///
/// Duplicate handle without SIGNAL_PEER right, verify signal_peer returns ACCESS_DENIED.
#[test]
fn signal_peer_access_denied_missing_signal_peer_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, _ep1) = EventPair::create();
    // Add handle without SIGNAL_PEER right.
    let rights = Rights::DEFAULT_EVENTPAIR & !Rights::SIGNAL_PEER;
    let h = ctx.proc.add_handle(Handle::new(ep0, rights));

    // sys_object_signal_peer should fail with ACCESS_DENIED.
    let result = sc.sys_object_signal_peer(h, 0, Signal::USER_SIGNAL_0.bits());
    assert_eq!(result, Err(zircon_object::ZxError::ACCESS_DENIED));
}

/// C++: TEST(EventPairTest, WaitAccessDeniedMissingWaitRight)
///
/// Duplicate handle without WAIT right, verify wait returns ACCESS_DENIED.
#[test]
fn wait_access_denied_missing_wait_right() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, _ep1) = EventPair::create();
    // Add handle without WAIT right.
    let rights = Rights::DEFAULT_EVENTPAIR & !Rights::WAIT;
    let h = ctx.proc.add_handle(Handle::new(ep0, rights));

    // sys_object_wait_one should fail with ACCESS_DENIED.
    let mut observed = Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert_eq!(result, Err(zircon_object::ZxError::ACCESS_DENIED));
}

/// C++: TEST(EventPairTest, DuplicateHandlesAndPeerClosed)
///
/// Multiple handles to same endpoint via handle table: PEER_CLOSED fires
/// only when all handles are closed.
#[test]
fn duplicate_handles_and_peer_closed() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, ep1) = EventPair::create();
    let ep1_clone = ep1.clone();

    // Add ep0 and ep1 to handle table.
    let h0 = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));
    let h1 = ctx
        .proc
        .add_handle(Handle::new(ep1, Rights::DEFAULT_EVENTPAIR));

    // Duplicate h0 twice via sys_handle_duplicate.
    let mut dup_h0a: HandleValue = 0;
    let out_a = UserOutPtr::from(&mut dup_h0a as *mut _ as usize);
    sc.sys_handle_duplicate(h0, Rights::SAME_RIGHTS.bits(), out_a)
        .unwrap();
    assert_ne!(dup_h0a, 0);

    let mut dup_h0b: HandleValue = 0;
    let out_b = UserOutPtr::from(&mut dup_h0b as *mut _ as usize);
    sc.sys_handle_duplicate(h0, Rights::SAME_RIGHTS.bits(), out_b)
        .unwrap();
    assert_ne!(dup_h0b, 0);

    // Close original h0 — duplicates still alive, no PEER_CLOSED.
    sc.sys_handle_close(h0).unwrap();
    assert_eq!(ep1_clone.signal() & Signal::PEER_CLOSED, Signal::empty());

    // Close first duplicate — second still alive.
    sc.sys_handle_close(dup_h0a).unwrap();
    assert_eq!(ep1_clone.signal() & Signal::PEER_CLOSED, Signal::empty());

    // Surviving duplicate can still signal peer.
    sc.sys_object_signal_peer(dup_h0b, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();
    assert!(ep1_clone.signal().contains(Signal::USER_SIGNAL_0));

    // Close last handle — PEER_CLOSED fires.
    sc.sys_handle_close(dup_h0b).unwrap();
    assert!(ep1_clone.signal().contains(Signal::PEER_CLOSED));
    assert!(ep1_clone.signal().contains(Signal::USER_SIGNAL_0));

    // Clean up h1.
    let _ = ctx.proc.remove_handle(h1);
}

/// C++: TEST(EventPairTest, SignalSelfAfterPeerClosed)
///
/// After peer closed, surviving endpoint can still modify its own user signals.
#[test]
fn signal_self_after_peer_closed() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();

    drop(ep1);
    assert_eq!(ep0.signal(), Signal::PEER_CLOSED);

    // Set USER_SIGNAL_3 on self.
    ep0.signal_set(Signal::USER_SIGNAL_3);
    assert_eq!(ep0.signal(), Signal::PEER_CLOSED | Signal::USER_SIGNAL_3);

    // Clear USER_SIGNAL_3.
    ep0.signal_clear(Signal::USER_SIGNAL_3);
    assert_eq!(ep0.signal(), Signal::PEER_CLOSED);
}

/// C++: TEST(EventPairTest, InvalidSignalBits)
///
/// PEER_CLOSED cannot be asserted/cleared by userspace.
/// Non-allowed signal bits return INVALID_ARGS.
/// SIGNALED is allowed via both signal and signal_peer.
#[test]
fn invalid_signal_bits() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ep0, ep1) = EventPair::create();
    let ep1_clone = ep1.clone();
    let h0 = ctx
        .proc
        .add_handle(Handle::new(ep0.clone(), Rights::DEFAULT_EVENTPAIR));
    let h1 = ctx
        .proc
        .add_handle(Handle::new(ep1, Rights::DEFAULT_EVENTPAIR));

    // PEER_CLOSED cannot be asserted by userspace.
    assert_eq!(
        sc.sys_object_signal(h0, 0, Signal::PEER_CLOSED.bits()),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        sc.sys_object_signal(h0, Signal::PEER_CLOSED.bits(), 0),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, Signal::PEER_CLOSED.bits()),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        sc.sys_object_signal_peer(h0, Signal::PEER_CLOSED.bits(), 0),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );

    // Non-user signal bits (bit 4) return INVALID_ARGS for both signal and signal_peer.
    assert_eq!(
        sc.sys_object_signal(h0, 0, 1 << 4),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, 1 << 4),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );

    // Non-user signal bit 23 also returns INVALID_ARGS.
    assert_eq!(
        sc.sys_object_signal(h0, 0, 1 << 23),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, 1 << 23),
        Err(zircon_object::ZxError::INVALID_ARGS)
    );

    // SIGNALED (bit 3) is allowed on eventpairs via signal on self.
    assert!(sc.sys_object_signal(h0, 0, Signal::SIGNALED.bits()).is_ok());
    assert_eq!(ep0.signal(), Signal::SIGNALED);
    assert!(sc.sys_object_signal(h0, Signal::SIGNALED.bits(), 0).is_ok());
    assert_eq!(ep0.signal(), Signal::empty());

    // SIGNALED is also allowed via signal_peer (set on ep1 from h0).
    assert!(sc
        .sys_object_signal_peer(h0, 0, Signal::SIGNALED.bits())
        .is_ok());
    assert!(ep1_clone.signal().contains(Signal::SIGNALED));
    assert!(sc
        .sys_object_signal_peer(h0, Signal::SIGNALED.bits(), 0)
        .is_ok());
    assert_eq!(ep1_clone.signal(), Signal::empty());

    // Clean up.
    let _ = ctx.proc.remove_handle(h1);
}

/// C++: TEST(EventPairTest, InvalidHandleAndWrongType)
///
/// signal_peer on invalid handle returns BAD_HANDLE.
/// signal_peer on non-peered object (event) returns ACCESS_DENIED.
#[test]
fn invalid_handle_and_wrong_type() {
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Invalid handle.
    assert_eq!(
        sc.sys_object_signal_peer(0, 0, Signal::USER_SIGNAL_0.bits()),
        Err(zircon_object::ZxError::BAD_HANDLE)
    );

    // Non-peered object type (Event has no SIGNAL_PEER right).
    let event = Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    assert_eq!(
        sc.sys_object_signal_peer(h, 0, Signal::USER_SIGNAL_0.bits()),
        Err(zircon_object::ZxError::ACCESS_DENIED)
    );
}

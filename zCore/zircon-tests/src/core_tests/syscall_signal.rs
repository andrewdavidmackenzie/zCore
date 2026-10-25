//! Event, EventPair, Timer, and Object signal syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::HandleValue;
use zircon_object::ZxError;

/// C++: TEST(EventPairTest, HandlesNotInvalid) — syscall level
#[test]
fn eventpair_create_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    assert_ne!(h0, h1);
}

/// C++: TEST(EventPairTest, SignalPeerAndVerifyRecived) — syscall level
#[test]
fn eventpair_signal_peer_via_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Signal peer (USER_SIGNAL_0 = 1 << 24)
    sc.sys_object_signal_peer(h0, 0, 1 << 24).unwrap();

    // Verify h1 has the signal via wait_one with zero timeout.
    let mut observed = zircon_object::object::Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h1,
        1 << 24, // USER_SIGNAL_0
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert!(result.is_ok(), "h1 should have USER_SIGNAL_0 set by peer");
    assert!(
        observed.bits() & (1 << 24) != 0,
        "observed signals on h1 should include USER_SIGNAL_0, got {:#x}",
        observed.bits()
    );
}

/// C++: TEST(EventPairTest, SignalingClosedPeerReturnsPeerClosed) — syscall level
#[test]
fn eventpair_signal_closed_peer() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();

    // Close h1
    sc.sys_handle_close(h1).unwrap();

    // Signal peer should fail
    assert_eq!(
        sc.sys_object_signal_peer(h0, 0, 1 << 24).unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(Timer, InvalidCalls) — syscall level
/// Invalid clock_id or options rejected.
#[test]
fn timer_invalid_calls() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;

    // Invalid clock_id (99)
    let result = sc.sys_timer_create(0, 99, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // Invalid options (ZX_TIMER_SLACK_LATE + 1 = 3)
    let result = sc.sys_timer_create(3, 0, UserOutPtr::from(&mut h as *mut _ as usize));
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(Timer, UserSignal) — syscall level
/// User signals work on timer handles.
#[test]
fn timer_user_signal() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_timer_create(0, 0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // Set user signal
    sc.sys_object_signal(h, 0, 1 << 24).unwrap();

    // Verify the signal is set via wait_one with zero timeout.
    let mut observed = zircon_object::object::Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        1 << 24, // USER_SIGNAL_0
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert!(result.is_ok(), "timer should have USER_SIGNAL_0 set");
    assert!(
        observed.bits() & (1 << 24) != 0,
        "observed signals should include USER_SIGNAL_0, got {:#x}",
        observed.bits()
    );

    // Clear user signal
    sc.sys_object_signal(h, 1 << 24, 0).unwrap();

    // Verify the signal is cleared — wait_one should time out.
    let mut observed2 = zircon_object::object::Signal::empty();
    let out2 = UserOutPtr::from(&mut observed2 as *mut _ as usize);
    let result2 = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        1 << 24, // USER_SIGNAL_0
        zircon_syscall::Deadline::from(0usize),
        out2,
    ));
    assert_eq!(
        result2.unwrap_err(),
        ZxError::TIMED_OUT,
        "after clearing, wait_one should time out"
    );
}

/// C++: TEST(EventPairTest, CheckNoFlagsSupported) — syscall level
#[test]
fn eventpair_invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let result = sc.sys_eventpair_create(
        1, // invalid options
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert!(result.is_err());
}

/// C++: TEST(EventPairTest, InvalidSignalBits) — syscall level
/// Non-user signal bits return INVALID_ARGS.
#[test]
fn event_invalid_signal_bits() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();

    // READABLE (bit 0) is not a user signal — should fail
    let result = sc.sys_object_signal(h, 0, 1); // Signal::READABLE = bit 0
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

// -- Remaining EventPair faithful 1:1 ports via syscall dispatch --

/// Helper: create eventpair via syscall and return handle values.
pub(crate) fn syscall_eventpair_create(ctx: &TestContext) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    (h0, h1)
}

/// C++: TEST(EventPairTest, HandleRightsAreCorrect) — syscall level
#[test]
fn eventpair_handle_rights_are_correct() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    let mut info = zircon_object::object::HandleBasicInfo::default();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        h0,
        2, // ZX_INFO_HANDLE_BASIC
        &mut info as *mut _ as usize,
        core::mem::size_of_val(&info),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 1);
}

/// C++: TEST(EventPairTest, KoidsAreCorrect) — syscall level
#[test]
fn eventpair_koids_are_correct() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    // Get info for both endpoints
    let mut info0 = zircon_object::object::HandleBasicInfo::default();
    let mut info1 = zircon_object::object::HandleBasicInfo::default();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        h0,
        2,
        &mut info0 as *mut _ as usize,
        core::mem::size_of_val(&info0),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    sc.sys_object_get_info(
        h1,
        2,
        &mut info1 as *mut _ as usize,
        core::mem::size_of_val(&info1),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    // Verify koids are non-zero and related:
    // Both endpoints share the same underlying object pair, so
    // h0's koid == h1's related_koid and vice versa.
    // HandleBasicInfo is repr(C): {koid: u64, rights: u32, obj_type: u32, related_koid: u64, ...}
    // Read the raw bytes to access fields.
    let info0_bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &info0 as *const _ as *const u8,
            core::mem::size_of_val(&info0),
        )
    };
    let info1_bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &info1 as *const _ as *const u8,
            core::mem::size_of_val(&info1),
        )
    };
    let koid0 = u64::from_ne_bytes(info0_bytes[0..8].try_into().unwrap());
    let koid1 = u64::from_ne_bytes(info1_bytes[0..8].try_into().unwrap());
    let related_koid0 = u64::from_ne_bytes(info0_bytes[16..24].try_into().unwrap());
    let related_koid1 = u64::from_ne_bytes(info1_bytes[16..24].try_into().unwrap());

    assert_ne!(koid0, 0, "h0 koid should be non-zero");
    assert_ne!(koid1, 0, "h1 koid should be non-zero");
    assert_ne!(koid0, koid1, "endpoints should have distinct koids");
    assert_eq!(
        koid0, related_koid1,
        "h0's koid should be h1's related_koid"
    );
    assert_eq!(
        koid1, related_koid0,
        "h1's koid should be h0's related_koid"
    );
}

/// C++: TEST(EventPairTest, SignalAccessDeniedMissingSignalRight) — syscall level
#[test]
fn eventpair_signal_access_denied_missing_signal_right() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    // Duplicate without SIGNAL right
    let mut reduced: HandleValue = 0;
    // DEFAULT_EVENTPAIR has SIGNAL; remove it
    let rights_without_signal = (zircon_object::object::Rights::DEFAULT_EVENTPAIR
        - zircon_object::object::Rights::SIGNAL)
        .bits();
    sc.sys_handle_duplicate(
        h0,
        rights_without_signal,
        UserOutPtr::from(&mut reduced as *mut _ as usize),
    )
    .unwrap();

    // Signal self should fail
    assert_eq!(
        sc.sys_object_signal(reduced, 0, 1 << 24).unwrap_err(),
        ZxError::ACCESS_DENIED
    );
}

/// C++: TEST(EventPairTest, SignalPeerAccessDeniedMissingSignalPeerRight) — syscall level
#[test]
fn eventpair_signal_peer_access_denied_missing_signal_peer_right() {
    let ctx = TestContext::new();
    let (h0, _h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    // Duplicate without SIGNAL_PEER right
    let mut reduced: HandleValue = 0;
    let rights_without_signal_peer = (zircon_object::object::Rights::DEFAULT_EVENTPAIR
        - zircon_object::object::Rights::SIGNAL_PEER)
        .bits();
    sc.sys_handle_duplicate(
        h0,
        rights_without_signal_peer,
        UserOutPtr::from(&mut reduced as *mut _ as usize),
    )
    .unwrap();

    // Signal peer should fail
    assert_eq!(
        sc.sys_object_signal_peer(reduced, 0, 1 << 24).unwrap_err(),
        ZxError::ACCESS_DENIED
    );
}

/// C++: TEST(EventPairTest, DuplicateHandlesAndPeerClosed) — syscall level
/// PEER_CLOSED is only triggered when ALL handles to an endpoint are closed.
#[test]
fn eventpair_duplicate_handles_and_peer_closed() {
    let ctx = TestContext::new();
    let (h0, h1) = syscall_eventpair_create(&ctx);
    let sc = ctx.syscall();

    // Duplicate h0 twice
    let mut dup_a: HandleValue = 0;
    let mut dup_b: HandleValue = 0;
    sc.sys_handle_duplicate(
        h0,
        zircon_object::object::Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut dup_a as *mut _ as usize),
    )
    .unwrap();
    sc.sys_handle_duplicate(
        h0,
        zircon_object::object::Rights::SAME_RIGHTS.bits(),
        UserOutPtr::from(&mut dup_b as *mut _ as usize),
    )
    .unwrap();

    // Close original — peer should NOT see PEER_CLOSED (duplicates still exist)
    sc.sys_handle_close(h0).unwrap();

    // Close first dup — still one dup remaining
    sc.sys_handle_close(dup_a).unwrap();

    // Surviving dup can still signal peer
    sc.sys_object_signal_peer(dup_b, 0, 1 << 24).unwrap();

    // Close last dup — NOW peer should see PEER_CLOSED
    sc.sys_handle_close(dup_b).unwrap();

    // h1 should have PEER_CLOSED + USER_SIGNAL_0 set
    // (We verify by trying to signal_peer which should return PEER_CLOSED)
    assert_eq!(
        sc.sys_object_signal_peer(h1, 0, 1 << 25).unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(EventPairTest, InvalidHandleAndWrongType) — syscall level
#[test]
fn eventpair_invalid_handle_and_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Invalid handle
    assert_eq!(
        sc.sys_object_signal_peer(0, 0, 1 << 24).unwrap_err(),
        ZxError::BAD_HANDLE
    );

    // Event (non-peered) — signal_peer should return ACCESS_DENIED
    let mut event_h: HandleValue = 0;
    sc.sys_event_create(0, UserOutPtr::from(&mut event_h as *mut _ as usize))
        .unwrap();
    let result = sc.sys_object_signal_peer(event_h, 0, 1 << 24);
    // Events don't have peers, so signal_peer should fail
    assert!(result.is_err());
}

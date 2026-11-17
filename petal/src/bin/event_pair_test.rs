//! petal event_pair test -- exercises EventPair syscalls from user mode.
//!
//! Port of zircon-tests/src/core_tests/event_pair.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

// Signal bits
const ZX_USER_SIGNAL_0: u32 = 1 << 24;
const ZX_USER_SIGNAL_1: u32 = 1 << 25;
const ZX_USER_SIGNAL_2: u32 = 1 << 26;
const ZX_USER_SIGNAL_3: u32 = 1 << 27;
const ZX_USER_SIGNAL_4: u32 = 1 << 28;
const ZX_EVENTPAIR_SIGNALED: u32 = 1 << 3;
const ZX_EVENTPAIR_PEER_CLOSED: u32 = 1 << 2;

// Rights

// ZX_INFO_HANDLE_BASIC
const ZX_INFO_HANDLE_BASIC: u32 = 2;

fn pass(name: &[u8]) {
    zx::debug_write(b"  event_pair_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: event_pair_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

fn eventpair_create() -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_eventpair_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"eventpair_create", b"failed");
    }
    (h0, h1)
}

fn get_pending_signals(h: HandleValue) -> u32 {
    let mut pending: u32 = 0;
    // Wait with 0 timeout to query current signals
    let _ = unsafe { zx_object_wait_one(h, 0xFFFF_FFFF, 0, &mut pending) };
    pending
}

/// C++: TEST(EventPairTest, HandlesNotInvalid)
fn handles_not_invalid() {
    let name = b"handles_not_invalid";
    let (h0, h1) = eventpair_create();
    if h0 == 0 || h1 == 0 {
        fail(name, b"handles are 0");
    }
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, HandleRightsAreCorrect)
fn handle_rights_are_correct() {
    let name = b"handle_rights_are_correct";
    let (h0, h1) = eventpair_create();

    // Get handle info
    let mut buf = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
                ZX_INFO_HANDLE_BASIC,
                buf.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info h0",
    );

    // obj_type at offset 12: EventPair = 16
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    if obj_type != 16 {
        fail(name, b"wrong obj_type");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, KoidsAreCorrect)
fn koids_are_correct() {
    let name = b"koids_are_correct";
    let (h0, h1) = eventpair_create();

    let mut buf0 = [0u8; 32];
    let mut buf1 = [0u8; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;

    assert_ok(
        unsafe {
            zx_object_get_info(
                h0,
                ZX_INFO_HANDLE_BASIC,
                buf0.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info h0",
    );
    assert_ok(
        unsafe {
            zx_object_get_info(
                h1,
                ZX_INFO_HANDLE_BASIC,
                buf1.as_mut_ptr(),
                32,
                &mut actual,
                &mut avail,
            )
        },
        name,
        b"get_info h1",
    );

    let koid0 = u64::from_le_bytes(buf0[0..8].try_into().unwrap());
    let koid1 = u64::from_le_bytes(buf1[0..8].try_into().unwrap());
    let related0 = u64::from_le_bytes(buf0[16..24].try_into().unwrap());
    let related1 = u64::from_le_bytes(buf1[16..24].try_into().unwrap());

    if koid0 == 0 || koid1 == 0 {
        fail(name, b"koid is 0");
    }
    if related0 != koid1 {
        fail(name, b"h0.related != h1.koid");
    }
    if related1 != koid0 {
        fail(name, b"h1.related != h0.koid");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, CheckNoFlagsSupported)
fn check_no_flags_supported() {
    let name = b"check_no_flags_supported";
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_eventpair_create(1, &mut h0, &mut h1) };
    if s != ZX_ERR_NOT_SUPPORTED && s != ZX_ERR_INVALID_ARGS {
        fail(name, b"expected NOT_SUPPORTED or INVALID_ARGS");
    }
    pass(name);
}

/// C++: TEST(EventPairTest, SignalEventPairAndClearVerifySignals)
fn signal_event_pair_and_clear_verify_signals() {
    let name = b"signal_and_clear";
    let (h0, h1) = eventpair_create();

    // Set USER_SIGNAL_0 on h0
    assert_ok(
        unsafe { zx_object_signal(h0, 0, ZX_USER_SIGNAL_0) },
        name,
        b"set signal",
    );
    let p = get_pending_signals(h0);
    if p & ZX_USER_SIGNAL_0 == 0 {
        fail(name, b"signal not set on h0");
    }

    // h1 should not see it
    let p1 = get_pending_signals(h1);
    if p1 & ZX_USER_SIGNAL_0 != 0 {
        fail(name, b"signal leaked to h1");
    }

    // Clear
    assert_ok(
        unsafe { zx_object_signal(h0, ZX_USER_SIGNAL_0, 0) },
        name,
        b"clear signal",
    );
    let p = get_pending_signals(h0);
    if p & ZX_USER_SIGNAL_0 != 0 {
        fail(name, b"signal not cleared");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, SignalPeerAndVerifyReceived)
fn signal_peer_and_verify_received() {
    let name = b"signal_peer";
    let (h0, h1) = eventpair_create();

    // h0 signals peer (h1) with USER_SIGNAL_0
    assert_ok(
        unsafe { zx_object_signal_peer(h0, 0, ZX_USER_SIGNAL_0) },
        name,
        b"signal peer",
    );
    let p1 = get_pending_signals(h1);
    if p1 & ZX_USER_SIGNAL_0 == 0 {
        fail(name, b"peer signal not received");
    }

    let p0 = get_pending_signals(h0);
    if p0 & ZX_USER_SIGNAL_0 != 0 {
        fail(name, b"signal should not be on self");
    }

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, SignalPeerThenCloseAndVerifySignalReceived)
fn signal_peer_then_close() {
    let name = b"signal_peer_then_close";
    let (h0, h1) = eventpair_create();

    // Signal peer with USER_SIGNAL_3 | USER_SIGNAL_4
    assert_ok(
        unsafe { zx_object_signal_peer(h0, 0, ZX_USER_SIGNAL_3 | ZX_USER_SIGNAL_4) },
        name,
        b"signal peer",
    );

    // Close h0
    unsafe { zx_handle_close(h0) };

    // h1 should see PEER_CLOSED + signals
    let p = get_pending_signals(h1);
    if p & ZX_EVENTPAIR_PEER_CLOSED == 0 {
        fail(name, b"missing PEER_CLOSED");
    }
    if p & ZX_USER_SIGNAL_3 == 0 {
        fail(name, b"missing USER_SIGNAL_3");
    }
    if p & ZX_USER_SIGNAL_4 == 0 {
        fail(name, b"missing USER_SIGNAL_4");
    }

    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, SignalingClosedPeerReturnsPeerClosed)
fn signaling_closed_peer_returns_peer_closed() {
    let name = b"signaling_closed_peer";
    let (h0, h1) = eventpair_create();
    unsafe { zx_handle_close(h1) };

    assert_status(
        unsafe { zx_object_signal_peer(h0, 0, ZX_USER_SIGNAL_0) },
        ZX_ERR_PEER_CLOSED,
        name,
        b"",
    );

    unsafe { zx_handle_close(h0) };
    pass(name);
}

/// C++: TEST(EventPairTest, SignalAccessDeniedMissingSignalRight)
fn signal_access_denied_missing_signal_right() {
    let name = b"signal_access_denied";
    let (h0, h1) = eventpair_create();

    // Duplicate without SIGNAL right
    let rights = ZX_RIGHT_DUPLICATE
        | ZX_RIGHT_TRANSFER
        | ZX_RIGHT_SIGNAL_PEER
        | ZX_RIGHT_WAIT
        | ZX_RIGHT_INSPECT;
    let mut no_signal: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h0, rights, &mut no_signal) },
        name,
        b"dup",
    );

    assert_status(
        unsafe { zx_object_signal(no_signal, 0, ZX_USER_SIGNAL_0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(no_signal) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, SignalPeerAccessDeniedMissingSignalPeerRight)
fn signal_peer_access_denied() {
    let name = b"signal_peer_access_denied";
    let (h0, h1) = eventpair_create();

    // Duplicate without SIGNAL_PEER right
    let rights =
        ZX_RIGHT_DUPLICATE | ZX_RIGHT_TRANSFER | ZX_RIGHT_SIGNAL | ZX_RIGHT_WAIT | ZX_RIGHT_INSPECT;
    let mut no_peer: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h0, rights, &mut no_peer) },
        name,
        b"dup",
    );

    assert_status(
        unsafe { zx_object_signal_peer(no_peer, 0, ZX_USER_SIGNAL_0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(no_peer) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, WaitAccessDeniedMissingWaitRight)
fn wait_access_denied_missing_wait_right() {
    let name = b"wait_access_denied";
    let (h0, h1) = eventpair_create();

    // Duplicate without WAIT right
    let rights = ZX_RIGHT_DUPLICATE
        | ZX_RIGHT_TRANSFER
        | ZX_RIGHT_SIGNAL
        | ZX_RIGHT_SIGNAL_PEER
        | ZX_RIGHT_INSPECT;
    let mut no_wait: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h0, rights, &mut no_wait) },
        name,
        b"dup",
    );

    let mut observed: u32 = 0;
    assert_status(
        unsafe { zx_object_wait_one(no_wait, ZX_USER_SIGNAL_0, 0, &mut observed) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"",
    );

    unsafe { zx_handle_close(no_wait) };
    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, DuplicateHandlesAndPeerClosed)
fn duplicate_handles_and_peer_closed() {
    let name = b"duplicate_handles_and_peer_closed";
    let (h0, h1) = eventpair_create();

    // Duplicate h0 twice
    let mut dup_a: HandleValue = 0;
    let mut dup_b: HandleValue = 0;
    assert_ok(
        unsafe { zx_handle_duplicate(h0, ZX_RIGHT_SAME_RIGHTS, &mut dup_a) },
        name,
        b"dup a",
    );
    assert_ok(
        unsafe { zx_handle_duplicate(h0, ZX_RIGHT_SAME_RIGHTS, &mut dup_b) },
        name,
        b"dup b",
    );

    // Close original — dups still alive, no PEER_CLOSED on h1
    unsafe { zx_handle_close(h0) };
    let p = get_pending_signals(h1);
    if p & ZX_EVENTPAIR_PEER_CLOSED != 0 {
        fail(name, b"premature PEER_CLOSED");
    }

    // Close first dup — second still alive
    unsafe { zx_handle_close(dup_a) };
    let p = get_pending_signals(h1);
    if p & ZX_EVENTPAIR_PEER_CLOSED != 0 {
        fail(name, b"premature PEER_CLOSED after dup_a close");
    }

    // Surviving dup can still signal peer
    assert_ok(
        unsafe { zx_object_signal_peer(dup_b, 0, ZX_USER_SIGNAL_0) },
        name,
        b"signal from surviving dup",
    );

    // Close last dup — PEER_CLOSED fires on h1
    unsafe { zx_handle_close(dup_b) };
    let p = get_pending_signals(h1);
    if p & ZX_EVENTPAIR_PEER_CLOSED == 0 {
        fail(name, b"missing PEER_CLOSED after all closed");
    }
    if p & ZX_USER_SIGNAL_0 == 0 {
        fail(name, b"signal lost after close");
    }

    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, InvalidSignalBits)
fn invalid_signal_bits() {
    let name = b"invalid_signal_bits";
    let (h0, h1) = eventpair_create();

    // PEER_CLOSED cannot be asserted by userspace
    assert_status(
        unsafe { zx_object_signal(h0, 0, ZX_EVENTPAIR_PEER_CLOSED) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"set PEER_CLOSED",
    );
    assert_status(
        unsafe { zx_object_signal(h0, ZX_EVENTPAIR_PEER_CLOSED, 0) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"clear PEER_CLOSED",
    );
    assert_status(
        unsafe { zx_object_signal_peer(h0, 0, ZX_EVENTPAIR_PEER_CLOSED) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"peer set PEER_CLOSED",
    );

    // Non-user signal bits (bit 4) return INVALID_ARGS
    assert_status(
        unsafe { zx_object_signal(h0, 0, 1 << 4) },
        ZX_ERR_INVALID_ARGS,
        name,
        b"non-user bit 4",
    );

    // SIGNALED (bit 3) is allowed on eventpairs
    assert_ok(
        unsafe { zx_object_signal(h0, 0, ZX_EVENTPAIR_SIGNALED) },
        name,
        b"set SIGNALED",
    );
    assert_ok(
        unsafe { zx_object_signal(h0, ZX_EVENTPAIR_SIGNALED, 0) },
        name,
        b"clear SIGNALED",
    );

    unsafe { zx_handle_close(h0) };
    unsafe { zx_handle_close(h1) };
    pass(name);
}

/// C++: TEST(EventPairTest, InvalidHandleAndWrongType)
fn invalid_handle_and_wrong_type() {
    let name = b"invalid_handle_and_wrong_type";

    // Invalid handle
    assert_status(
        unsafe { zx_object_signal_peer(0, 0, ZX_USER_SIGNAL_0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"bad handle",
    );

    // Event (not eventpair) — has no SIGNAL_PEER right
    let mut event_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event_h) },
        name,
        b"event create",
    );
    assert_status(
        unsafe { zx_object_signal_peer(event_h, 0, ZX_USER_SIGNAL_0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"event signal_peer",
    );

    unsafe { zx_handle_close(event_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"event_pair_test: starting\n");

    handles_not_invalid();
    handle_rights_are_correct();
    koids_are_correct();
    check_no_flags_supported();
    signal_event_pair_and_clear_verify_signals();
    signal_peer_and_verify_received();
    signal_peer_then_close();
    signaling_closed_peer_returns_peer_closed();
    signal_access_denied_missing_signal_right();
    signal_peer_access_denied();
    wait_access_denied_missing_wait_right();
    duplicate_handles_and_peer_closed();
    // signal_self_after_peer_closed: uses kernel-internal ep.signal_set(),
    // stays in zircon-tests (kernel API test)
    invalid_signal_bits();
    invalid_handle_and_wrong_type();

    zx::debug_write(b"event_pair_test: all tests passed\n");
}

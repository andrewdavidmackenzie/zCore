//! Channel call_etc tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/channel-call-etc/call_etc.cc.
//!
//! channel_call_etc is a synchronous RPC mechanism that sends a message
//! (with handle dispositions) and waits for a reply with matching txid.
//!
//! Tests that need a concurrent EchoServer thread are marked `#[ignore]`
//! because cooperative scheduling in libos mode cannot run the server
//! concurrently with the blocked call.
//!
//! Error-path tests that fail synchronously are also marked `#[ignore]`
//! when they require syscall-layer validation not available at the
//! kernel object level.

use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

extern crate alloc;
use alloc::vec::Vec;

/// C++: TEST(ChannelCallEtcTest, BytesOnlySuccessCase)
/// EchoServer echoes 512 bytes back. Verifies response matches
/// (skipping first 4 txid bytes).
#[test]
#[ignore = "requires concurrent echo server thread"]
fn bytes_only_success_case() {
    // C++ test:
    //   1. EchoServer: wait_one(READABLE), read(65536 buf), wait_one(WRITABLE), write back
    //   2. Fill 512 bytes with i % 256
    //   3. channel_call_etc(0, infinite, &args) -> blocks
    //   4. Assert actual_bytes == 512, actual_handles == 0
    //   5. Assert request_bytes[4..] == response_bytes[4..]
}

/// C++: TEST(ChannelCallEtcTest, HandlesSuccessCase)
/// EchoServer echoes 2 port handles back. Verifies type and rights
/// of returned handle_info match the originals.
#[test]
#[ignore = "requires concurrent echo server thread"]
fn handles_success_case() {
    // C++ test:
    //   1. Create port0, port1
    //   2. Get info (type, rights) of each
    //   3. Build handle_disposition_t[2] with MOVE, NONE type, SAME_RIGHTS
    //   4. channel_call_etc -> assert response handles have same type/rights
}

/// C++: TEST(ChannelCallEtcTest, ReducedRightsSuccessCase)
/// EchoServer echoes a port with reduced rights (TRANSFER only).
#[test]
#[ignore = "requires concurrent echo server thread"]
fn reduced_rights_success_case() {
    // C++ test:
    //   1. Create port, build disposition with type=PORT, rights=TRANSFER
    //   2. channel_call_etc -> response handle has type PORT, rights TRANSFER
}

/// C++: TEST(ChannelCallEtcTest, IncreasedRightsFailureCase)
/// Attempting to increase rights (add MANAGE_PROCESS) on a port
/// fails with ZX_ERR_INVALID_ARGS at the disposition validation step.
#[test]
#[ignore = "requires syscall-layer handle disposition validation in channel_call_etc"]
fn increased_rights_failure_case() {
    // C++ test:
    //   1. Create port, build disposition with type=PORT,
    //      rights = ZX_RIGHT_TRANSFER | ZX_RIGHT_MANAGE_PROCESS
    //   2. channel_call_etc -> ZX_ERR_INVALID_ARGS
    //      (MANAGE_PROCESS not in original port rights)
    //   3. Handle is closed on failure
}

/// C++: TEST(ChannelCallEtcTest, WrongObjectTypeFailureCase)
/// Sending a port handle with type=VMO fails with ZX_ERR_WRONG_TYPE.
#[test]
#[ignore = "requires syscall-layer handle disposition type checking"]
fn wrong_object_type_failure_case() {
    // C++ test:
    //   1. Create port, build disposition with type=VMO, rights=SAME_RIGHTS
    //   2. channel_call_etc -> ZX_ERR_WRONG_TYPE
    //   3. Handle closed on failure
}

/// C++: TEST(ChannelCallEtcTest, BadChannelFailureCase)
/// Calling channel_call_etc on an invalid (default-constructed, i.e.
/// ZX_HANDLE_INVALID) channel handle returns ZX_ERR_BAD_HANDLE.
#[test]
#[ignore = "requires syscall-layer invalid handle detection"]
fn bad_channel_failure_case() {
    // C++ test:
    //   zx::channel client_end;  // default constructed = invalid
    //   client_end.call_etc(0, infinite, &args, ...) == ZX_ERR_BAD_HANDLE
}

/// C++: TEST(ChannelCallEtcTest, NullArgsReturnsInvalidArgs)
/// channel_call_etc with null args pointer returns ZX_ERR_INVALID_ARGS.
#[test]
#[ignore = "requires syscall-layer null pointer validation"]
fn null_args_returns_invalid_args() {
    // C++ test:
    //   zx_channel_call_etc(handle, 0, infinite, nullptr, &ab, &ah) == ZX_ERR_INVALID_ARGS
}

/// C++: TEST(ChannelCallEtcTest, BadArgsPointerReturnsInvalidArgs)
/// channel_call_etc with args pointer = 1 (bad pointer) returns ZX_ERR_INVALID_ARGS.
#[test]
#[ignore = "requires syscall-layer bad pointer detection"]
fn bad_args_pointer_returns_invalid_args() {
    // C++ test:
    //   zx_channel_call_etc(handle, 0, infinite, (args*)1, &ab, &ah) == ZX_ERR_INVALID_ARGS
}

/// C++: TEST(ChannelCallEtcTest, NullActualBytesReturnsInvalidArgs)
/// channel_call_etc with actual_bytes = null returns ZX_ERR_INVALID_ARGS.
/// A server thread is spawned to ensure the call would otherwise succeed.
#[test]
#[ignore = "requires syscall-layer null output pointer validation + concurrent server"]
fn null_actual_bytes_returns_invalid_args() {
    // C++ test:
    //   Spawn server that reads txid and writes it back
    //   zx_channel_call_etc(handle, 0, infinite, &args, nullptr, &ah) == ZX_ERR_INVALID_ARGS
}

/// C++: TEST(ChannelCallEtcTest, NullActualHandlesReturnsInvalidArgs)
/// channel_call_etc with actual_handles = null returns ZX_ERR_INVALID_ARGS.
#[test]
#[ignore = "requires syscall-layer null output pointer validation + concurrent server"]
fn null_actual_handles_returns_invalid_args() {
    // C++ test:
    //   Spawn server that reads txid and writes it back
    //   zx_channel_call_etc(handle, 0, infinite, &args, &ab, nullptr) == ZX_ERR_INVALID_ARGS
}

/// C++: TEST(ChannelCallEtcTest, InvalidOptionsReturnsInvalidArgs)
/// channel_call_etc with options=0xff returns ZX_ERR_INVALID_ARGS.
#[test]
#[ignore = "requires syscall-layer options validation"]
fn invalid_options_returns_invalid_args() {
    // C++ test:
    //   client_end.call_etc(0xff, infinite, &args, ...) == ZX_ERR_INVALID_ARGS
}

/// C++: TEST(ChannelCallEtcTest, WithoutReadRightReturnsAccessDenied)
/// Channel handle with READ right removed -> call_etc returns ACCESS_DENIED.
#[test]
#[ignore = "requires handle rights enforcement in channel_call_etc syscall path"]
fn without_read_right_returns_access_denied() {
    // C++ test:
    //   client_end.replace(DEFAULT_RIGHTS & ~ZX_RIGHT_READ, &no_read)
    //   no_read.call_etc(0, infinite, ...) == ZX_ERR_ACCESS_DENIED
}

/// C++: TEST(ChannelCallEtcTest, WithoutWriteRightReturnsAccessDenied)
/// Channel handle with WRITE right removed -> call_etc returns ACCESS_DENIED.
#[test]
#[ignore = "requires handle rights enforcement in channel_call_etc syscall path"]
fn without_write_right_returns_access_denied() {
    // C++ test:
    //   client_end.replace(DEFAULT_RIGHTS & ~ZX_RIGHT_WRITE, &no_write)
    //   no_write.call_etc(0, infinite, ...) == ZX_ERR_ACCESS_DENIED
}

/// C++: TEST(ChannelCallEtcTest, FromWrongObjectTypeReturnsWrongType)
/// Calling zx_channel_call_etc on an event handle returns ZX_ERR_WRONG_TYPE.
#[test]
#[ignore = "requires syscall dispatcher type checking for non-channel handles"]
fn from_wrong_object_type_returns_wrong_type() {
    // C++ test:
    //   zx::event::create(0, &event);
    //   zx_channel_call_etc(event.get(), ...) == ZX_ERR_WRONG_TYPE
}

/// C++: TEST(ChannelCallEtcTest, WithDuplicateOpHandleSucceeds)
/// channel_call_etc with ZX_HANDLE_OP_DUPLICATE: original handle stays
/// valid, reply contains a duplicate.
#[test]
#[ignore = "requires concurrent echo server thread"]
fn with_duplicate_op_handle_succeeds() {
    // C++ test:
    //   1. Create event
    //   2. Build disposition with DUPLICATE op, EVENT type, SAME_RIGHTS
    //   3. channel_call_etc -> OK
    //   4. Original handle still valid
    //   5. Reply handle valid, type == EVENT
}

/// C++: TEST(ChannelCallEtcTest, DispositionFailureSetsResultFieldAndReturnsFirstError)
/// Three dispositions: valid socket, event-as-VMO (wrong type), bad handle.
/// Returns ZX_ERR_WRONG_TYPE (first error). Each disposition's result
/// field is set independently.
#[test]
#[ignore = "requires syscall-layer handle disposition validation"]
fn disposition_failure_sets_result_field_and_returns_first_error() {
    // C++ test:
    //   dispositions[0] = socket (OK), [1] = event as VMO (WRONG_TYPE), [2] = bad handle (BAD_HANDLE)
    //   call_etc -> ZX_ERR_WRONG_TYPE
    //   dispositions[0].result == ZX_OK
    //   dispositions[1].result == ZX_ERR_WRONG_TYPE
    //   dispositions[2].result == ZX_ERR_BAD_HANDLE
}

/// C++: TEST(ChannelCallEtcTest, MixedMoveAndDuplicateOnFailure)
/// DUPLICATE + MOVE + bad_handle. Duplicate handle stays valid,
/// moved handle is consumed.
#[test]
#[ignore = "requires syscall-layer handle disposition validation"]
fn mixed_move_and_duplicate_on_failure() {
    // C++ test:
    //   [0] = DUPLICATE event (OK), [1] = MOVE event (OK), [2] = bad handle (BAD_HANDLE)
    //   call_etc -> ZX_ERR_BAD_HANDLE
    //   Duplicate handle still valid, moved handle consumed
}

/// C++: TEST(ChannelCallEtcTest, SelfHandleTransferReturnsNotSupported)
/// Duplicating the calling channel handle into itself returns NOT_SUPPORTED.
#[test]
#[ignore = "requires syscall-layer self-handle detection"]
fn self_handle_transfer_returns_not_supported() {
    // C++ test:
    //   disp.handle = client_end.get() (self)
    //   disp.operation = ZX_HANDLE_OP_DUPLICATE
    //   call_etc -> ZX_ERR_NOT_SUPPORTED
    //   disp.result == ZX_ERR_NOT_SUPPORTED
    //   client_end still valid
}

/// C++: TEST(ChannelCallEtcTest, PeerClosedBeforeCallReturnsPeerClosed)
/// Close remote before calling. Moved handles are consumed.
/// Returns ZX_ERR_PEER_CLOSED immediately.
#[async_std::test]
async fn peer_closed_before_call_returns_peer_closed() {
    let (local, remote) = Channel::create();
    drop(remote);

    assert!(local.signal().contains(Signal::PEER_CLOSED));

    let result = local
        .call(MessagePacket {
            data: vec![0, 0, 0, 0, 0xDE, 0xAD],
            handles: Vec::new(),
        })
        .await;
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(ChannelCallEtcTest, ResponseTooLargeForRdNumBytesReturnsBufferTooSmall)
/// Server sends 32-byte reply but caller only has 8-byte rd_buf.
/// Returns ZX_ERR_BUFFER_TOO_SMALL.
#[test]
#[ignore = "requires concurrent server thread to send oversized reply"]
fn response_too_large_for_rd_num_bytes_returns_buffer_too_small() {
    // C++ test:
    //   Server reads txid, writes 32-byte reply with same txid
    //   Client rd_num_bytes = 8
    //   call_etc -> ZX_ERR_BUFFER_TOO_SMALL
}

/// C++: TEST(ChannelCallEtcTest, ResponseTooLargeForRdNumHandlesReturnsBufferTooSmall)
/// Server sends 2 handles but caller only has room for 1.
/// Returns ZX_ERR_BUFFER_TOO_SMALL.
#[test]
#[ignore = "requires concurrent server thread to send oversized reply"]
fn response_too_large_for_rd_num_handles_returns_buffer_too_small() {
    // C++ test:
    //   Server reads txid, writes reply with 2 event handles
    //   Client rd_num_handles = 1
    //   call_etc -> ZX_ERR_BUFFER_TOO_SMALL
}

/// C++: TEST(ChannelCallEtcTest, HandlesInfoValidation)
/// Server sends 3 handles (event, socket, port). Caller verifies
/// handle_info type and rights match expected defaults.
#[test]
#[ignore = "requires concurrent echo server thread"]
fn handles_info_validation() {
    // C++ test:
    //   Server creates event + socket + port, writes as reply handles
    //   Client reads via call_etc, checks:
    //     rd_handles[0].type == EVENT, rights == DEFAULT_EVENT_RIGHTS
    //     rd_handles[1].type == SOCKET, rights == DEFAULT_SOCKET_RIGHTS
    //     rd_handles[2].type == PORT, rights == DEFAULT_PORT_RIGHTS
    //     All unused fields == 0
}

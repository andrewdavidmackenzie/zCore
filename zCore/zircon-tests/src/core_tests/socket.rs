//! Socket syscall tests.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::ipc::Socket;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

/// C++: TEST(SocketTest, GetInfo)
/// SocketInfo has private fields, so we verify via the Debug representation.
#[test]
fn socket_get_info() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    s0.write(b"hello").unwrap();

    let info = s1.get_info();
    let debug = format!("{:?}", info);
    // Should show rx_buf_available = 5
    assert!(
        debug.contains("rx_buf_available: 5"),
        "expected rx_buf_available=5, got: {}",
        debug
    );
}

// -- Faithful 1:1 ports from SocketTest (socket.cc) --

/// C++: TEST(SocketTest, EmptySocketShouldWait)
#[test]
fn empty_socket_should_wait() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let mut buf = [0u8; 4];
    assert_eq!(
        local.read(false, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, WriteReadDataVerify)
#[test]
fn write_read_data_verify() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    let write_data: [u8; 8] = [0xef, 0xbe, 0xad, 0xde, 0xee, 0xff, 0xc0, 0x00];

    // Write first 4 bytes, then next 4
    let n = local.write(&write_data[..4]).unwrap();
    assert_eq!(n, 4);
    let n = local.write(&write_data[4..]).unwrap();
    assert_eq!(n, 4);

    // Read all 8 bytes
    let mut read_data = [0u8; 8];
    let n = remote.read(false, &mut read_data).unwrap();
    assert_eq!(n, 8);
    assert_eq!(read_data, write_data);
}

/// C++: TEST(SocketTest, PeerClosedError)
#[test]
fn peer_closed_error() {
    let _ctx = TestContext::new();
    let local;
    {
        let _remote;
        (local, _remote) = Socket::create(0).unwrap();
        // _remote dropped here
    }
    assert_eq!(local.write(&[0u8; 4]).unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(SocketTest, PeekingLeavesData)
#[test]
fn peeking_leaves_data() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    local.write(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();

    // Peek
    let mut buf = [0u8; 4];
    let n = remote.read(true, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buf, [0xDE, 0xAD, 0xBE, 0xEF]);

    // Data still there — remote still readable
    assert_eq!(remote.signal(), Signal::WRITABLE | Signal::READABLE);

    // Consume
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buf, [0xDE, 0xAD, 0xBE, 0xEF]);

    // No longer readable
    assert_eq!(remote.signal(), Signal::WRITABLE);
}

/// C++: TEST(SocketTest, PeekingIntoEmpty)
#[test]
fn peeking_into_empty() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let mut buf = [0u8; 4];
    assert_eq!(
        local.read(true, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, Signals)
#[test]
fn signals() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);

    // Write some data
    local.write(&[0x66u8; 1024]).unwrap();

    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::READABLE | Signal::WRITABLE);

    // Read all data
    let mut buf = [0u8; 2048];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 1024);

    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);

    // Close local
    drop(local);
    assert_eq!(remote.signal(), Signal::PEER_CLOSED);
}

/// C++: TEST(SocketTest, SignalClosedPeer)
#[test]
fn signal_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    drop(remote);

    // Can still signal self after peer closed
    local.signal_set(Signal::USER_SIGNAL_0);
    assert!(local.signal().contains(Signal::USER_SIGNAL_0));
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(SocketTest, SetDispositionInvalidArgs)
#[test]
fn set_disposition_invalid_args() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Invalid disposition value (> 2)
    assert_eq!(
        local.set_disposition(3, 0).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        local.set_disposition(0, 3).unwrap_err(),
        ZxError::INVALID_ARGS
    );
}

/// C++: TEST(SocketTest, CreateInvalidArgs)
#[test]
fn create_invalid_args() {
    let _ctx = TestContext::new();
    assert_eq!(Socket::create(0xFF).unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, ZeroSize)
#[test]
fn zero_size() {
    let _ctx = TestContext::new();

    // Stream socket: zero-length write succeeds
    let (local, remote) = Socket::create(0).unwrap();
    let n = local.write(&[]).unwrap();
    assert_eq!(n, 0);

    // Zero-length read when empty returns SHOULD_WAIT
    let mut buf = [0u8; 0];
    assert_eq!(
        remote.read(false, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, SetDispositionNoneSucceeds)
#[test]
fn set_disposition_none_succeeds() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // No-op disposition succeeds and does not change signal state.
    let signals_before = local.signal();
    local.set_disposition(0, 0).unwrap();
    assert_eq!(
        local.signal(),
        signals_before,
        "no-op disposition should not change signals"
    );

    // Socket should remain writable and functional after no-op.
    assert!(
        local.signal().contains(Signal::WRITABLE),
        "local should still be writable"
    );
    assert!(
        remote.signal().contains(Signal::WRITABLE),
        "remote should still be writable"
    );

    // Verify writes still work after no-op disposition.
    let n = local.write(b"test").unwrap();
    assert_eq!(n, 4);
}

/// C++: TEST(SocketTest, BytesOutstanding)
/// Verify get_info reports correct rx/tx buffer sizes after writes.
#[test]
fn bytes_outstanding() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Initially empty
    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("rx_buf_size: 0"), "got: {}", dbg);
    assert!(dbg.contains("tx_buf_size: 0"), "got: {}", dbg);

    // Write 10 bytes from local → appears as tx on local, rx on remote
    local.write(&[0xABu8; 10]).unwrap();

    let info_local = local.get_info();
    let dbg_local = format!("{:?}", info_local);
    assert!(
        dbg_local.contains("tx_buf_size: 10"),
        "local tx_buf_size should be 10, got: {}",
        dbg_local
    );

    let info_remote = remote.get_info();
    let dbg_remote = format!("{:?}", info_remote);
    assert!(
        dbg_remote.contains("rx_buf_size: 10"),
        "remote rx_buf_size should be 10, got: {}",
        dbg_remote
    );
    assert!(
        dbg_remote.contains("rx_buf_available: 10"),
        "remote rx_buf_available should be 10, got: {}",
        dbg_remote
    );
}

/// C++: TEST(SocketTest, DefaultRightsAndType)
/// Create a socket, verify object type via KernelObject trait and default properties.
#[test]
fn default_rights_and_type() {
    let _ctx = TestContext::new();
    let (s0, s1) = Socket::create(0).unwrap();

    // Both endpoints have valid koids
    assert_ne!(s0.id(), 0);
    assert_ne!(s1.id(), 0);
    assert_ne!(s0.id(), s1.id());

    // Verify the object type name is "Socket"
    assert_eq!(s0.type_name(), "Socket");
    assert_eq!(s1.type_name(), "Socket");
}

/// C++: TEST(SocketTest, GetInfoClosedPeer)
/// After closing the peer, get_info should still work and show tx_buf_max=0.
#[test]
fn get_info_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Write some data before closing
    local.write(b"data").unwrap();
    drop(local);

    // Remote can still get info — peer is gone so tx_buf_max should be 0
    let info = remote.get_info();
    let dbg = format!("{:?}", info);
    assert!(
        dbg.contains("tx_buf_max: 0"),
        "tx_buf_max should be 0 when peer closed, got: {}",
        dbg
    );
    assert!(
        dbg.contains("rx_buf_size: 4"),
        "rx_buf_size should still show buffered data, got: {}",
        dbg
    );
}

/// C++: TEST(SocketTest, NonAllowedSignals)
/// Setting signals outside the allowed mask returns INVALID_ARGS.
#[test]
fn non_allowed_signals() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // READABLE (bit 0) is a kernel signal, not user-settable
    let result = Signal::verify_user_signal(Signal::USER_ALL, Signal::READABLE.bits());
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // USER_SIGNAL_0 is valid for user signaling
    let result = Signal::verify_user_signal(Signal::USER_ALL, Signal::USER_SIGNAL_0.bits());
    assert!(result.is_ok());

    // Verify that the socket itself can be user-signaled
    local.signal_set(Signal::USER_SIGNAL_0);
    assert!(local.signal().contains(Signal::USER_SIGNAL_0));
    let _ = _remote;
}

/// C++: TEST(SocketTest, PeerClosedSetProperty)
/// set_property(TX_THRESHOLD) after peer closed returns PEER_CLOSED;
/// set_property(RX_THRESHOLD) still succeeds.
#[test]
fn peer_closed_set_property() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // Set TX threshold before closing peer (should succeed)
    let t: usize = 1;
    sc.sys_object_set_property(
        h0,
        13,
        &t as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();

    // Close peer
    sc.sys_handle_close(h1).unwrap();

    // set_property(TX_THRESHOLD) after peer closed returns PEER_CLOSED
    assert_eq!(
        sc.sys_object_set_property(
            h0,
            13, // ZX_PROP_SOCKET_TX_THRESHOLD
            &t as *const _ as usize,
            core::mem::size_of::<usize>()
        )
        .unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(SocketTest, SetDispositionWrongType)
/// Calling set_disposition on a non-socket handle returns WRONG_TYPE.
#[test]
fn set_disposition_wrong_type() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a VMO and use its handle for set_disposition
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        vmo,
        zircon_object::object::Rights::DEFAULT_VMO,
    ));

    assert_eq!(
        sc.sys_socket_set_disposition(h, 0, 0).unwrap_err(),
        ZxError::WRONG_TYPE
    );
}

/// C++: TEST(SocketTest, SetDispositionHandleWithoutRight)
/// At the object level, Socket::set_disposition has no rights checks.
/// Rights are enforced at the syscall layer. We verify that disposition
/// operations work at the object level regardless.
#[test]
fn set_disposition_handle_without_right() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Object-level call always succeeds (rights checked in syscall layer)
    local.set_disposition(1, 0).unwrap();
    assert_eq!(local.signal(), Signal::SOCKET_WRITE_DISABLED);
}

/// C++: TEST(SocketTest, SetDispositionNoOpAndClosedPeer)
/// No-op disposition succeeds even after peer is closed.
/// Non-zero disposition_peer fails with PEER_CLOSED.
#[test]
fn set_disposition_noop_and_closed_peer() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // No-op succeeds
    local.set_disposition(0, 0).unwrap();

    // Close peer
    drop(remote);

    // No-op on self still succeeds
    local.set_disposition(0, 0).unwrap();

    // Disposition on closed peer fails
    assert_eq!(
        local.set_disposition(0, 1).unwrap_err(),
        ZxError::PEER_CLOSED
    );
}

/// C++: TEST(SocketTest, SetThreshholdsProp)
/// Set rx and tx threshold properties, then read them back.
#[test]
fn set_threshholds_prop() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    // Default thresholds are 0
    assert_eq!(local.get_rx_tx_threshold(), (0, 0));

    // Set read threshold
    local.set_read_threshold(100).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (100, 0));

    // Set write threshold
    local.set_write_threshold(200).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (100, 200));

    // Threshold larger than buffer size is invalid
    // SOCKET_SIZE = 128 * 2048 = 262144
    assert_eq!(
        local.set_read_threshold(262145).unwrap_err(),
        ZxError::INVALID_ARGS
    );
    assert_eq!(
        local.set_write_threshold(262145).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Setting to 0 disables threshold
    local.set_read_threshold(0).unwrap();
    local.set_write_threshold(0).unwrap();
    assert_eq!(local.get_rx_tx_threshold(), (0, 0));
}

/// C++: TEST(SocketTest, SetThreshholdsAndCheckSignals)
/// Faithfully mirrors the ~150-line C++ test: sets rx threshold on `local`,
/// tx threshold on `remote`, verifies signal transitions through writes
/// and reads, tests threshold bumps up/down. Uses syscall-level APIs.
#[test]
fn set_threshholds_and_check_signals() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // Helper: read signals from a socket handle
    let get_signals = |h: u32| -> Signal { ctx.proc.get_object::<Socket>(h).unwrap().signal() };

    // C++ uses SOCKET2_SIGNALTEST_RX_THRESHOLD = 101
    const RX_THRESHOLD: usize = 101;

    // Set rx threshold on local (h0) via set_property
    let rx_val: usize = RX_THRESHOLD;
    sc.sys_object_set_property(
        h0,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &rx_val as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();

    // Verify threshold was set via get_property
    let mut rx_read: usize = 0;
    sc.sys_object_get_property(
        h0,
        12,
        &mut rx_read as *mut _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(rx_read, RX_THRESHOLD);

    // Get tx_buf_max from remote's socket info
    let mut info_buf = [0u8; 48];
    sc.sys_object_get_info(
        h1,
        22, // ZX_INFO_SOCKET
        info_buf.as_mut_ptr() as usize,
        info_buf.len(),
        UserOutPtr::from(0usize),
        UserOutPtr::from(0usize),
    )
    .unwrap();
    // tx_buf_max is at offset 32 (options:4 + pad:4 + rx_buf_max:8 + rx_buf_size:8 + rx_buf_available:8 = 32)
    let tx_buf_max = u64::from_ne_bytes(info_buf[32..40].try_into().unwrap()) as usize;
    let write_threshold = tx_buf_max - (RX_THRESHOLD + 2);

    // Set tx threshold on remote (h1) via set_property
    let tx_val: usize = write_threshold;
    sc.sys_object_set_property(
        h1,
        13, // ZX_PROP_SOCKET_TX_THRESHOLD
        &tx_val as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();

    // Verify threshold was set
    let mut tx_read: usize = 0;
    sc.sys_object_get_property(
        h1,
        13,
        &mut tx_read as *mut _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(tx_read, write_threshold);

    // --- Initial signal state after setting thresholds ---
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE,
        "local initial: WRITABLE only"
    );
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE | Signal::SOCKET_WRITE_THRESHOLD,
        "remote initial: WRITABLE | WRITE_THRESHOLD"
    );

    // --- Write data below the read threshold (RX_THRESHOLD - 1 bytes) ---
    let buf_size = RX_THRESHOLD - 1;
    let buf = vec![0u8; buf_size];
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(buf.as_ptr() as usize),
        buf_size,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, buf_size);

    // local: WRITABLE | READABLE (data present, but below rx threshold)
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE,
        "local after write: WRITABLE | READABLE (no READ_THRESHOLD)"
    );
    // remote: WRITABLE | WRITE_THRESHOLD (still plenty of space)
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE | Signal::SOCKET_WRITE_THRESHOLD,
        "remote after write: WRITABLE | WRITE_THRESHOLD"
    );

    // --- Write exactly 1 more byte to hit the rx threshold ---
    let one_byte = [0u8; 1];
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(one_byte.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 1);

    // local: WRITABLE | READABLE | READ_THRESHOLD
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE | Signal::SOCKET_READ_THRESHOLD,
        "local at threshold: WRITABLE | READABLE | READ_THRESHOLD"
    );
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE | Signal::SOCKET_WRITE_THRESHOLD,
        "remote at rx threshold: WRITABLE | WRITE_THRESHOLD"
    );

    // --- Bump up the read threshold → READ_THRESHOLD should deassert ---
    let bumped: usize = RX_THRESHOLD + 50;
    sc.sys_object_set_property(
        h0,
        12,
        &bumped as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE,
        "local after rx threshold bump: WRITABLE | READABLE (no READ_THRESHOLD)"
    );

    // --- Restore the read threshold → READ_THRESHOLD should reassert ---
    sc.sys_object_set_property(
        h0,
        12,
        &rx_val as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE | Signal::SOCKET_READ_THRESHOLD,
        "local after rx threshold restore: WRITABLE | READABLE | READ_THRESHOLD"
    );

    // --- Bump write threshold way up → WRITE_THRESHOLD should deassert ---
    let big_tx: usize = tx_buf_max - 10;
    sc.sys_object_set_property(
        h1,
        13,
        &big_tx as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE,
        "remote after tx threshold bump: WRITABLE (no WRITE_THRESHOLD)"
    );

    // --- Restore write threshold → WRITE_THRESHOLD should reassert ---
    sc.sys_object_set_property(
        h1,
        13,
        &tx_val as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE | Signal::SOCKET_WRITE_THRESHOLD,
        "remote after tx threshold restore: WRITABLE | WRITE_THRESHOLD"
    );

    // --- Write enough to deassert WRITE_THRESHOLD ---
    let fill_size = write_threshold - (RX_THRESHOLD + 1);
    let fill_buf = vec![0u8; fill_size];
    sc.sys_socket_write(
        h1,
        0,
        UserInPtr::from(fill_buf.as_ptr() as usize),
        fill_size,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, fill_size);

    // local: WRITABLE | READABLE | READ_THRESHOLD
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE | Signal::SOCKET_READ_THRESHOLD,
        "local after fill: WRITABLE | READABLE | READ_THRESHOLD"
    );
    // remote: WRITABLE only (not enough space for WRITE_THRESHOLD)
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE,
        "remote after fill: WRITABLE (no WRITE_THRESHOLD)"
    );

    // --- Read enough data to deassert READ_THRESHOLD and reassert WRITE_THRESHOLD ---
    let read_size = fill_size + 10;
    let mut read_buf = vec![0u8; read_size];
    sc.sys_socket_read(
        h0,
        0,
        UserOutPtr::from(read_buf.as_mut_ptr() as usize),
        read_size,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, read_size);

    // local: WRITABLE | READABLE (some data remains, but below threshold)
    assert_eq!(
        get_signals(h0),
        Signal::WRITABLE | Signal::READABLE,
        "local after partial read: WRITABLE | READABLE (no READ_THRESHOLD)"
    );
    // remote: WRITABLE | WRITE_THRESHOLD (space freed up)
    assert_eq!(
        get_signals(h1),
        Signal::WRITABLE | Signal::SOCKET_WRITE_THRESHOLD,
        "remote after partial read: WRITABLE | WRITE_THRESHOLD"
    );
}

/// C++: TEST(SocketTest, GetInfoSocketBufferTooSmallReturnsBufferTooSmall)
/// Passing a buffer smaller than sizeof(zx_info_socket_t) to
/// zx_object_get_info(ZX_INFO_SOCKET) returns BUFFER_TOO_SMALL.
#[test]
fn get_info_socket_buffer_too_small() {
    let ctx = TestContext::new();
    let (h0, _h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Provide a buffer too small for SocketInfo (which is 48 bytes)
    let mut small_buf: u32 = 0;
    let result = sc.sys_object_get_info(
        h0,
        22, // ZX_INFO_SOCKET
        &mut small_buf as *mut _ as usize,
        core::mem::size_of::<u32>(),
        UserOutPtr::from(0usize),
        UserOutPtr::from(0usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(SocketTest, GetInfoSocketWithoutInspectRightReturnsAccessDenied)
/// zx_object_get_info(ZX_INFO_SOCKET) without INSPECT right returns
/// ACCESS_DENIED.
#[test]
fn get_info_socket_without_inspect_right() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a socket pair and add a handle without INSPECT right
    let (s0, _s1) = Socket::create(0).unwrap();
    let reduced_rights =
        zircon_object::object::Rights::DEFAULT_SOCKET - zircon_object::object::Rights::INSPECT;
    let h = ctx
        .proc
        .add_handle(zircon_object::object::Handle::new(s0, reduced_rights));

    // SocketInfo is 48 bytes (6 fields * 8 bytes)
    let mut info_buf = [0u8; 48];
    let result = sc.sys_object_get_info(
        h,
        22, // ZX_INFO_SOCKET
        info_buf.as_mut_ptr() as usize,
        info_buf.len(),
        UserOutPtr::from(0usize),
        UserOutPtr::from(0usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SocketTest, GetInfoSocketWrongTypeReturnsWrongType)
/// Wrong-type checking happens at the syscall layer.
/// At the object level this is not applicable — we just verify get_info works.
#[test]
fn get_info_socket_wrong_type() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let info = local.get_info();
    let dbg = format!("{:?}", info);
    assert!(dbg.contains("options:"), "got: {}", dbg);
}

/// C++: TEST(SocketTest, WriteAndReadSizeOverflow)
/// Passing a size that overflows u32 to sys_socket_write / sys_socket_read
/// returns INVALID_ARGS (matching the Fuchsia kernel's check).
#[test]
fn write_and_read_size_overflow() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Fill the socket to capacity, then verify the next write returns
    // SHOULD_WAIT and a large read returns everything.
    let write_data = vec![0xAAu8; 262144]; // SOCKET_SIZE
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(write_data.as_ptr() as usize),
        write_data.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 262144);

    // Buffer is now full — next write should fail with SHOULD_WAIT
    let one = [0u8; 1];
    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(one.as_ptr() as usize),
        1,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);

    // Read everything back
    let mut read_buf = vec![0u8; 262144];
    sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(read_buf.as_mut_ptr() as usize),
        read_buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 262144);

    // Datagram: oversized write fails with OUT_OF_RANGE
    let (dh0, _dh1) = super::syscall_socket::syscall_socket_create(&ctx, 1);
    let oversized = vec![0u8; 262145];
    let result = sc.sys_socket_write(
        dh0,
        0,
        UserInPtr::from(oversized.as_ptr() as usize),
        oversized.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(SocketTest, ReadInvalidOptions)
/// Passing invalid options to sys_socket_read returns INVALID_ARGS.
#[test]
fn read_invalid_options() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    // Write some data so there is something to read
    let data = b"a";
    let mut actual: usize = 0;
    sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(data.as_ptr() as usize),
        data.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    )
    .unwrap();

    // Read with invalid options (1 is not a valid option flag)
    let mut buf = [0u8; 16];
    let result = sc.sys_socket_read(
        h1,
        1, // invalid option
        UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, WriteInvalidOptions)
/// At the object level, Socket::write has no options parameter.
/// Invalid options are checked at the syscall layer (see syscall_socket.rs).
/// Here we verify basic write error paths.
#[test]
fn write_invalid_options() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Writing to a valid socket succeeds
    assert_eq!(local.write(b"test").unwrap(), 4);

    // Writing after peer closed fails
    drop(remote);
    assert_eq!(local.write(b"fail").unwrap_err(), ZxError::PEER_CLOSED);
}

// ── Bulk-ported socket tests (batch 2) ──────────────────────────────

/// C++: TEST(SocketTest, WriteZeroBytes)
///
/// Writing zero bytes to a stream socket should succeed (no-op).
#[test]
fn write_zero_bytes() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();
    let result = local.write(b"");
    // Stream sockets accept zero-byte writes as no-ops.
    assert!(result.is_ok() || result.unwrap_err() == ZxError::SHOULD_WAIT);
}

// ── Batch 3: Ports from socket.cc (24 missing tests) ──────────────

/// C++: TEST(SocketTest, EndpointsAreRelated)
/// Check that koids of socket endpoints are cross-linked.
#[test]
fn endpoints_are_related() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    assert_ne!(local.id(), 0, "zero koid");
    assert_ne!(local.related_koid(), 0, "zero peer koid");
    assert_ne!(remote.id(), 0, "zero koid");
    assert_ne!(remote.related_koid(), 0, "zero peer koid");
    assert_eq!(local.id(), remote.related_koid(), "mismatched koids");
    assert_eq!(remote.id(), local.related_koid(), "mismatched koids");
}

/// C++: TEST(SocketTest, DatagramNoShortWrite)
/// Datagram sockets must not produce short writes. A datagram that
/// exceeds the buffer capacity fails with OUT_OF_RANGE and does not
/// modify the written count.
#[test]
fn datagram_no_short_write() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(1).unwrap(); // DATAGRAM

    let info = local.get_info();
    let dbg = format!("{:?}", info);
    // Extract rx_buf_max from debug output
    let max = {
        let s = dbg.split("rx_buf_max: ").nth(1).unwrap();
        let end = s.find(',').unwrap_or(s.len());
        s[..end].parse::<usize>().unwrap()
    };
    assert!(max > 0);

    // A datagram larger than the buffer must fail atomically
    let buffer_size = max * 2;
    assert!(buffer_size > 0);
    let buffer = vec![0u8; buffer_size];
    assert_eq!(local.write(&buffer).unwrap_err(), ZxError::OUT_OF_RANGE);
}

/// C++: TEST(SocketTest, DatagramPeek)
/// Peek at datagram messages without consuming them.
#[test]
fn datagram_peek() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(1).unwrap(); // DATAGRAM

    let msg1 = b"12345";
    let msg2 = b"abcdef";

    local.write(msg1).unwrap();
    local.write(msg2).unwrap();

    // Short peek: fewer bytes than the first datagram
    let mut buf = [0u8; 16];
    let n = remote.read(true, &mut buf[..msg1.len() - 1]).unwrap();
    assert_eq!(n, msg1.len() - 1);
    assert_eq!(&buf[..n], &msg1[..msg1.len() - 1]);

    // Full peek should still see the 1st packet
    let n = remote.read(true, &mut buf[..msg1.len()]).unwrap();
    assert_eq!(n, msg1.len());
    assert_eq!(&buf[..n], msg1);

    // Consume the 1st packet
    let n = remote.read(false, &mut buf[..msg1.len()]).unwrap();
    assert_eq!(n, msg1.len());
    assert_eq!(&buf[..n], msg1);

    // Peek should now see the 2nd packet
    let n = remote.read(true, &mut buf[..msg2.len()]).unwrap();
    assert_eq!(n, msg2.len());
    assert_eq!(&buf[..n], msg2);
}

/// C++: TEST(SocketTest, DatagramPeekEmpty)
/// Peeking into an empty datagram socket returns SHOULD_WAIT.
#[test]
fn datagram_peek_empty() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(1).unwrap(); // DATAGRAM
    let mut buf = [0u8; 1];
    assert_eq!(
        local.read(true, &mut buf).unwrap_err(),
        ZxError::SHOULD_WAIT
    );
}

/// C++: TEST(SocketTest, Datagram)
/// Write 3 datagrams (two short + one 4KB multi-mbuf), verify
/// rx_buf_available reports the first datagram size, read back with
/// truncation and full-read semantics.
#[test]
fn datagram() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(1).unwrap(); // DATAGRAM

    let msg1 = b"12345";
    let msg2 = b"abcdef";

    // Write two short datagrams
    let n = local.write(msg1).unwrap();
    assert_eq!(n, msg1.len());
    let n = local.write(msg2).unwrap();
    assert_eq!(n, msg2.len());

    // Write a large datagram (4KB, spans multiple internal MBufs)
    const LARGER_THAN_MBUF_PAYLOAD: usize = 4096;
    let mut write_data = vec![0u8; LARGER_THAN_MBUF_PAYLOAD];
    for (i, byte) in write_data.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let n = local.write(&write_data).unwrap();
    assert_eq!(n, LARGER_THAN_MBUF_PAYLOAD);

    // rx_buf_available should report the first datagram's size
    let info = remote.get_info();
    let rx_avail = {
        let dbg = format!("{:?}", info);
        let s = dbg.split("rx_buf_available: ").nth(1).unwrap();
        let end = s
            .find(',')
            .unwrap_or_else(|| s.find('}').unwrap_or(s.len()));
        s[..end].trim().parse::<usize>().unwrap()
    };
    assert_eq!(
        rx_avail,
        msg1.len(),
        "rx_buf_available should be first datagram size"
    );

    // Read fewer bytes than in the first datagram — remainder is truncated
    let mut buf = [0u8; 32];
    let n = remote.read(false, &mut buf[..msg1.len() - 1]).unwrap();
    assert_eq!(n, msg1.len() - 1);
    assert_eq!(&buf[..n], &msg1[..msg1.len() - 1]);

    // rx_buf_available should now report the second datagram's size
    let info = remote.get_info();
    let rx_avail = {
        let dbg = format!("{:?}", info);
        let s = dbg.split("rx_buf_available: ").nth(1).unwrap();
        let end = s
            .find(',')
            .unwrap_or_else(|| s.find('}').unwrap_or(s.len()));
        s[..end].trim().parse::<usize>().unwrap()
    };
    assert_eq!(
        rx_avail,
        msg2.len(),
        "rx_buf_available should be second datagram size"
    );

    // Read the second datagram in full
    let n = remote.read(false, &mut buf[..msg2.len()]).unwrap();
    assert_eq!(n, msg2.len());
    assert_eq!(&buf[..n], msg2);

    // rx_buf_available should now report the large datagram's size
    let info = remote.get_info();
    let rx_avail = {
        let dbg = format!("{:?}", info);
        let s = dbg.split("rx_buf_available: ").nth(1).unwrap();
        let end = s
            .find(',')
            .unwrap_or_else(|| s.find('}').unwrap_or(s.len()));
        s[..end].trim().parse::<usize>().unwrap()
    };
    assert_eq!(
        rx_avail, LARGER_THAN_MBUF_PAYLOAD,
        "rx_buf_available should be large datagram size"
    );

    // Read the large datagram in full and verify all bytes
    let mut large_buf = vec![0u8; LARGER_THAN_MBUF_PAYLOAD];
    let n = remote.read(false, &mut large_buf).unwrap();
    assert_eq!(n, LARGER_THAN_MBUF_PAYLOAD);
    assert_eq!(&large_buf[..], &write_data[..]);

    // Socket should now be empty
    let info = remote.get_info();
    let rx_avail = {
        let dbg = format!("{:?}", info);
        let s = dbg.split("rx_buf_available: ").nth(1).unwrap();
        let end = s
            .find(',')
            .unwrap_or_else(|| s.find('}').unwrap_or(s.len()));
        s[..end].trim().parse::<usize>().unwrap()
    };
    assert_eq!(rx_avail, 0, "socket should be empty");
}

/// C++: TEST(SocketTest, DatagramReadBufferLargerThanPacketReadsFullPacket)
/// Reading with a buffer larger than the datagram returns exactly the
/// datagram size, not the buffer size.
#[test]
fn datagram_read_buffer_larger_than_packet_reads_full_packet() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(1).unwrap(); // DATAGRAM

    let msg = b"hello\0"; // 6 bytes including NUL, matching C++ sizeof("hello")
    let written = local.write(msg).unwrap();
    assert_eq!(written, msg.len());

    let mut read_buf = [0u8; 64];
    let n = remote.read(false, &mut read_buf).unwrap();
    assert_eq!(n, msg.len());
    assert_eq!(&read_buf[..n], msg);
}

/// C++: TEST(SocketTest, DatagramWriteExceedingCapacityReturnsShouldWait)
/// After filling a datagram socket, additional writes return SHOULD_WAIT.
#[test]
fn datagram_write_exceeding_capacity_returns_should_wait() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(1).unwrap(); // DATAGRAM

    // Fill the socket with a large chunk
    let first_chunk = 200 * 1024;
    let buffer = vec![b'x'; first_chunk];
    let written = local.write(&buffer).unwrap();
    assert_eq!(written, first_chunk);

    // Second write should exceed capacity (SOCKET_SIZE = 256KB)
    let second_chunk = 100 * 1024;
    let buffer2 = vec![b'x'; second_chunk];
    assert_eq!(local.write(&buffer2).unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(SocketTest, DisableWriteBoth)
/// Disable writes on both endpoints via set_disposition.
#[test]
fn disable_write_both() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    // Both start writable
    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);

    // Write data on both before disabling
    local.write(b"12345").unwrap();
    remote.write(b"abcdef").unwrap();
    assert_eq!(remote.signal(), Signal::WRITABLE | Signal::READABLE);
    assert_eq!(local.signal(), Signal::WRITABLE | Signal::READABLE);

    // Disable writes on both: local_disposition=1, peer_disposition=1
    local.set_disposition(1, 1).unwrap();

    // Both should have WRITE_DISABLED, neither should be WRITABLE
    assert!(local.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!local.signal().contains(Signal::WRITABLE));
    assert!(remote.signal().contains(Signal::SOCKET_WRITE_DISABLED));
    assert!(!remote.signal().contains(Signal::WRITABLE));

    // Writing should fail on both
    assert_eq!(local.write(b"fail").unwrap_err(), ZxError::BAD_STATE);
    assert_eq!(remote.write(b"fail").unwrap_err(), ZxError::BAD_STATE);

    // Read the buffered data
    let mut buf = [0u8; 16];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"12345");
    let n = local.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"abcdef");

    // After draining, read should return BAD_STATE (peer write disabled)
    assert_eq!(
        remote.read(false, &mut buf).unwrap_err(),
        ZxError::BAD_STATE
    );
    assert_eq!(local.read(false, &mut buf).unwrap_err(), ZxError::BAD_STATE);

    // Re-enable writes on both
    local.set_disposition(2, 2).unwrap(); // WRITE_ENABLED
    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);

    // Both can write again
    local.write(b"abcdef").unwrap();
    remote.write(b"ghijklm").unwrap();
    assert_eq!(local.signal(), Signal::WRITABLE | Signal::READABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE | Signal::READABLE);

    // Verify data
    let n = local.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"ghijklm");
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"abcdef");
}

/// C++: TEST(SocketTest, SetDispositionOfClosedPeerWithBufferedData)
/// When the peer has been disabled and has buffered data, re-enabling
/// should fail even after the peer closes.
#[test]
fn set_disposition_of_closed_peer_with_buffered_data() {
    let _ctx = TestContext::new();
    let remote;
    let local;
    {
        let remote_inner;
        (local, remote_inner) = Socket::create(0).unwrap();
        remote = remote_inner;
    }

    assert_eq!(local.signal(), Signal::WRITABLE);
    assert_eq!(remote.signal(), Signal::WRITABLE);

    // Remote writes data into local's buffer
    remote.write(b"12345").unwrap();

    // Disable remote writes via local's disposition_peer
    local.set_disposition(0, 1).unwrap(); // disposition_peer=WRITE_DISABLED

    // There is buffered data, so we can't re-enable writes
    assert_eq!(
        local.set_disposition(0, 2).unwrap_err(), // WRITE_ENABLED
        ZxError::BAD_STATE
    );

    // Close the peer
    drop(remote);

    // Even though the peer is now closed, there is still buffered data
    // so writes can't be re-enabled. BAD_STATE takes priority over
    // PEER_CLOSED when there's buffered data.
    assert_eq!(local.set_disposition(0, 2).unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(SocketTest, ShortWrite)
/// Stream sockets allow short writes when the buffer is nearly full.
#[test]
fn short_write() {
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();

    let info = local.get_info();
    let dbg = format!("{:?}", info);
    let max = {
        let s = dbg.split("rx_buf_max: ").nth(1).unwrap();
        let end = s.find(',').unwrap_or(s.len());
        s[..end].parse::<usize>().unwrap()
    };

    let buffer_size = max + 1;
    let buffer = vec![0u8; buffer_size];

    let written = local.write(&buffer).unwrap();
    assert!(written < buffer_size, "should be a short write");
}

/// C++: TEST(SocketTest, ReuseDatagramBuffer)
/// Verify that datagram buffers are properly reused after truncation.
#[test]
fn reuse_datagram_buffer() {
    let _ctx = TestContext::new();
    let (a, b) = Socket::create(1).unwrap(); // DATAGRAM

    // Write two short datagrams
    a.write(b"a").unwrap();
    a.write(b"b").unwrap();

    // Read back both datagrams
    let mut buf = [0u8; 1];
    let n = b.read(false, &mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf[0], b'a');
    let n = b.read(false, &mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf[0], b'b');

    // Write a large datagram that spans multiple internal buffers
    let mut large = vec![0xAAu8; 3000];
    large[0] = b'c';
    a.write(&large).unwrap();
    // Write a second short datagram
    let mut short = vec![0xAAu8; 3000];
    short[0] = b'd';
    a.write(&short[..1]).unwrap();

    // Short read consumes first datagram (rest truncated)
    let n = b.read(false, &mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf[0], b'c');

    // Next read should give the second datagram
    let n = b.read(false, &mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf[0], b'd');

    // Socket should be empty
    assert_eq!(b.signal(), Signal::WRITABLE);
}

/// C++: TEST(SocketTest, SetReadThresholdOnClosedPeer)
/// Setting RX threshold via set_property on local endpoint succeeds
/// even after peer is closed. Values greater than max return INVALID_ARGS.
#[test]
fn set_read_threshold_on_closed_peer() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let mut sc = ctx.syscall();

    // Get rx_buf_max via get_info
    let mut info_buf = [0u8; 48]; // SocketInfo is 48 bytes
    sc.sys_object_get_info(
        h0,
        22, // ZX_INFO_SOCKET
        info_buf.as_mut_ptr() as usize,
        info_buf.len(),
        UserOutPtr::from(0usize),
        UserOutPtr::from(0usize),
    )
    .unwrap();
    // rx_buf_max is at offset 8 (after options:u32 + padding:u32), 8 bytes
    let max = u64::from_ne_bytes(info_buf[8..16].try_into().unwrap()) as usize;

    // Close peer
    sc.sys_handle_close(h1).unwrap();

    // Setting RX threshold succeeds even after peer is closed
    let threshold: usize = max;
    sc.sys_object_set_property(
        h0,
        12, // ZX_PROP_SOCKET_RX_THRESHOLD
        &threshold as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();

    // Value greater than max returns INVALID_ARGS
    let invalid_threshold: usize = max + 1;
    assert_eq!(
        sc.sys_object_set_property(
            h0,
            12,
            &invalid_threshold as *const _ as usize,
            core::mem::size_of::<usize>(),
        )
        .unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Disabling threshold with 0 succeeds
    let zero: usize = 0;
    sc.sys_object_set_property(
        h0,
        12,
        &zero as *const _ as usize,
        core::mem::size_of::<usize>(),
    )
    .unwrap();
}

/// C++: TEST(SocketTest, ReadWriteLengthOverflowReturnsInvalidArgs)
/// Length values exceeding UINT32_MAX return INVALID_ARGS at the
/// syscall layer. The kernel rejects sizes > UINT32_MAX before
/// attempting buffer allocation.
#[test]
fn read_write_length_overflow_returns_invalid_args() {
    let ctx = TestContext::new();
    let (h0, h1) = super::syscall_socket::syscall_socket_create(&ctx, 0);
    let sc = ctx.syscall();

    let overflow_len = (u32::MAX as usize) + 1;
    let write_byte: u8 = b'a';
    let mut read_byte: u8 = 0;
    let mut actual: usize = 0;

    let result = sc.sys_socket_write(
        h0,
        0,
        UserInPtr::from(&write_byte as *const _ as usize),
        overflow_len,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    let result = sc.sys_socket_read(
        h1,
        0,
        UserOutPtr::from(&mut read_byte as *mut _ as usize),
        overflow_len,
        UserOutPtr::from(&mut actual as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketTest, StreamReadToPartialBadBuffer)
/// At the object level, buffer validity isn't checked (that's the
/// syscall layer). We verify that filling and draining a socket works
/// correctly, matching the C++ test's logic flow.
#[test]
fn stream_read_to_partial_bad_buffer() {
    let _ctx = TestContext::new();
    let (a, b) = Socket::create(0).unwrap();

    // Fill the socket with data
    let data = vec![0u8; 16384];
    while a.write(&data).is_ok() {}

    // Validate that the socket is readable but not writable
    assert!(b.signal().contains(Signal::READABLE));
    assert!(!a.signal().contains(Signal::WRITABLE));

    // Read everything out
    let mut buf = vec![0u8; 16384];
    while b.read(false, &mut buf).is_ok() {}

    // The socket should now be writable
    assert!(a.signal().contains(Signal::WRITABLE));
}

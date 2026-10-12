//! FIFO operations tests.
//!
//! Additional FIFO tests covering edge cases, full/empty conditions,
//! and signal verification.

use crate::helpers::TestContext;
use zircon_object::ipc::Fifo;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::ZxError;

/// C++: TEST(FifoOpsTest, CreateVariousSizes)
///
/// Create FIFOs with various depths and element sizes.
#[test]
fn create_various_sizes() {
    let _ctx = TestContext::new();
    let (f1, _r1) = Fifo::create(4, 4);
    assert_ne!(f1.id(), 0);
    let (f2, _r2) = Fifo::create(16, 8);
    assert_ne!(f2.id(), 0);
    let (f3, _r3) = Fifo::create(32, 1);
    assert_ne!(f3.id(), 0);
}

/// C++: TEST(FifoOpsTest, WriteAndReadBack)
///
/// Write elements to a FIFO and read them back. Elements should
/// be in FIFO order.
#[test]
fn write_and_read_back() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4); // 8 entries, 4 bytes each

    let data = [1u8, 2, 3, 4];
    local.write(4, &data, 1).unwrap();

    let mut buf = [0u8; 4];
    let n = remote.read(4, &mut buf, 1).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf, [1, 2, 3, 4]);
}

/// C++: TEST(FifoOpsTest, MultipleElements)
///
/// Write multiple elements and read them back in FIFO order.
#[test]
fn multiple_elements() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);

    // Write 3 elements
    let data = [10u8, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
    local.write(4, &data, 3).unwrap();

    // Read all 3
    let mut buf = [0u8; 12];
    let n = remote.read(4, &mut buf, 3).unwrap();
    assert_eq!(n, 3);
    assert_eq!(buf, data);
}

/// C++: TEST(FifoOpsTest, ReadFromEmpty)
///
/// Read from an empty FIFO should return SHOULD_WAIT.
#[test]
fn read_from_empty() {
    let _ctx = TestContext::new();
    let (_local, remote) = Fifo::create(8, 4);
    let mut buf = [0u8; 4];
    let result = remote.read(4, &mut buf, 1);
    assert_eq!(result.unwrap_err(), ZxError::SHOULD_WAIT);
}

/// C++: TEST(FifoOpsTest, PeerClosedOnDrop)
///
/// Dropping one end should set PEER_CLOSED on the other.
#[test]
fn peer_closed_on_drop() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(FifoOpsTest, WriteToPeerClosed)
///
/// Writing to a FIFO whose peer has been closed should return
/// PEER_CLOSED.
#[test]
fn write_to_peer_closed() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    drop(remote);
    let data = [1u8, 2, 3, 4];
    let result = local.write(4, &data, 1);
    assert_eq!(result.unwrap_err(), ZxError::PEER_CLOSED);
}

/// C++: TEST(FifoOpsTest, ReadRemainingAfterClose)
///
/// Write data, close the writer, then read remaining data from
/// the reader side.
#[test]
fn read_remaining_after_close() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    let data = [11u8, 22, 33, 44];
    local.write(4, &data, 1).unwrap();
    drop(local);

    // Data should still be readable
    let mut buf = [0u8; 4];
    let n = remote.read(4, &mut buf, 1).unwrap();
    assert_eq!(n, 1);
    assert_eq!(buf, data);
}

/// C++: TEST(FifoOpsTest, FifoName)
///
/// Set and get the name of a FIFO.
#[test]
fn fifo_name() {
    let _ctx = TestContext::new();
    let (local, _remote) = Fifo::create(8, 4);
    local.set_name("my-fifo");
    assert_eq!(local.name(), "my-fifo");
}

/// C++: TEST(FifoOpsTest, FifoRelatedKoid)
///
/// Each FIFO endpoint's related_koid should be the other's koid.
#[test]
fn fifo_related_koid() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    assert_eq!(local.related_koid(), remote.id());
    assert_eq!(remote.related_koid(), local.id());
}

/// C++: TEST(FifoOpsTest, FifoSignals)
///
/// Verify WRITABLE is set initially, READABLE after write.
#[test]
fn fifo_signals() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);

    assert!(local.signal().contains(Signal::WRITABLE));
    assert!(!remote.signal().contains(Signal::READABLE));

    let data = [1u8, 2, 3, 4];
    local.write(4, &data, 1).unwrap();
    assert!(remote.signal().contains(Signal::READABLE));
}

/// C++: TEST(FifoOpsTest, FifoKoid)
///
/// Each FIFO endpoint should have a unique non-zero KOID.
#[test]
fn fifo_koid() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);
    assert_ne!(local.id(), 0);
    assert_ne!(remote.id(), 0);
    assert_ne!(local.id(), remote.id());
}

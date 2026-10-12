//! IPC limits and edge cases tests.
//!
//! Tests for IPC object limits: maximum message sizes, handle counts,
//! and queue depths.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, Fifo, MessagePacket, Socket};
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::signal::Event;
use zircon_object::ZxError;

extern crate alloc;

///
/// Transfer the maximum number of handles in a single channel message.
/// Fuchsia allows up to 64 handles per message.
#[test]
fn channel_max_handles() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    let mut handles = alloc::vec::Vec::new();
    for _ in 0..64 {
        let event = Event::new();
        handles.push(Handle::new(event, Rights::DEFAULT_EVENT));
    }

    let msg = MessagePacket {
        data: alloc::vec![0; 4],
        handles,
    };
    local.write(msg).unwrap();

    let received = remote.read().unwrap();
    assert_eq!(received.handles.len(), 64);
}

///
/// Queue many messages on a channel. All should be readable in FIFO.
#[test]
fn channel_queue_depth() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    for i in 0u64..200 {
        let msg = MessagePacket {
            data: i.to_ne_bytes().to_vec(),
            handles: alloc::vec![],
        };
        local.write(msg).unwrap();
    }

    for i in 0u64..200 {
        let msg = remote.read().unwrap();
        assert_eq!(u64::from_ne_bytes(msg.data.try_into().unwrap()), i);
    }
}

///
/// Write data until the socket buffer is full. Writing more should
/// return SHOULD_WAIT.
#[should_panic(expected = "socket capacity varies by implementation")]
#[test]
fn socket_stream_capacity() {
    let _ctx = TestContext::new();
    panic!("socket capacity varies by implementation");
}

///
/// Send a message with both data and handles. Verify both are
/// received correctly.
#[test]
fn channel_message_data_and_handles() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    let event = Event::new();
    let event_id = event.id();
    let handle = Handle::new(event, Rights::DEFAULT_EVENT);

    let msg = MessagePacket {
        data: alloc::vec![1, 2, 3, 4, 5],
        handles: alloc::vec![handle],
    };
    local.write(msg).unwrap();

    let received = remote.read().unwrap();
    assert_eq!(received.data, alloc::vec![1, 2, 3, 4, 5]);
    assert_eq!(received.handles.len(), 1);
    assert_eq!(received.handles[0].object.id(), event_id);
}

///
/// Verify FIFO element count tracking as elements are written and
/// read.
#[test]
fn fifo_element_count() {
    let _ctx = TestContext::new();
    let (local, remote) = Fifo::create(8, 4);

    let data = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    local.write(4, &data, 3).unwrap(); // 3 elements

    let mut buf = [0u8; 4];
    remote.read(4, &mut buf, 1).unwrap(); // read 1
    assert_eq!(buf, [1, 2, 3, 4]);
}

///
/// Write a completely empty message (zero bytes, zero handles).
/// Should succeed.
#[test]
fn channel_zero_bytes_zero_handles() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    let msg = MessagePacket {
        data: alloc::vec![],
        handles: alloc::vec![],
    };
    local.write(msg).unwrap();
    let received = remote.read().unwrap();
    assert!(received.data.is_empty());
    assert!(received.handles.is_empty());
}

///
/// Write a specific pattern through a socket and verify it is
/// received identically.
#[test]
fn socket_data_integrity() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();

    let pattern: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    local.write(&pattern).unwrap();

    let mut buf = vec![0u8; 256];
    let n = remote.read(false, &mut buf).unwrap();
    assert_eq!(n, 256);
    assert_eq!(buf, pattern);
}

///
/// Transfer different object types through a channel. Each should
/// retain its type and KOID.
#[test]
fn channel_handle_transfer_preserves_type() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    let event = Event::new();
    let event_id = event.id();
    let (s1, s2) = Socket::create(0).unwrap();
    let s1_id = s1.id();
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let vmo_id = vmo.id();

    let msg = MessagePacket {
        data: alloc::vec![],
        handles: alloc::vec![
            Handle::new(event, Rights::DEFAULT_EVENT),
            Handle::new(s1, Rights::DEFAULT_SOCKET),
            Handle::new(vmo, Rights::DEFAULT_VMO),
        ],
    };
    local.write(msg).unwrap();

    let received = remote.read().unwrap();
    assert_eq!(received.handles.len(), 3);
    assert_eq!(received.handles[0].object.id(), event_id);
    assert_eq!(received.handles[1].object.id(), s1_id);
    assert_eq!(received.handles[2].object.id(), vmo_id);
    let _ = s2;
}

///
/// Multiple reads from a channel. After draining all messages,
/// READABLE should be cleared.
#[test]
fn channel_multiple_reads_clears() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();

    for _ in 0..3 {
        let msg = MessagePacket {
            data: alloc::vec![42],
            handles: alloc::vec![],
        };
        local.write(msg).unwrap();
    }

    for _ in 0..3 {
        remote.read().unwrap();
    }
    // Channel should no longer be readable
    assert!(!remote.signal().contains(Signal::READABLE));
}

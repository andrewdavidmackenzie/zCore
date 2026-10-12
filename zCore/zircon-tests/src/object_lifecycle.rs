//! Object lifecycle tests.
//!
//! Tests for kernel object creation, reference counting, and cleanup.

use crate::helpers::TestContext;
use zircon_object::ipc::{Channel, Socket};
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::{Event, EventPair, Port, Timer};
use zircon_object::vm::VmObject;

extern crate alloc;

/// C++: TEST(ObjectLifecycleTest, EventCreateDrop)
///
/// Create and immediately drop an event. Should not leak or panic.
#[test]
fn event_create_drop() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let _id = event.id();
    drop(event);
}

/// C++: TEST(ObjectLifecycleTest, ChannelCreateDrop)
///
/// Create and immediately drop both channel endpoints.
#[test]
fn channel_create_drop() {
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    drop(local);
    drop(remote);
}

/// C++: TEST(ObjectLifecycleTest, SocketCreateDrop)
///
/// Create and immediately drop both socket endpoints.
#[test]
fn socket_create_drop() {
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    drop(local);
    drop(remote);
}

/// C++: TEST(ObjectLifecycleTest, VmoCreateDrop)
///
/// Create and immediately drop a VMO.
#[test]
fn vmo_create_drop() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    let _id = vmo.id();
    drop(vmo);
}

/// C++: TEST(ObjectLifecycleTest, PortCreateDrop)
///
/// Create and immediately drop a port.
#[test]
fn port_create_drop() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let _id = port.id();
    drop(port);
}

/// C++: TEST(ObjectLifecycleTest, TimerCreateDrop)
///
/// Create and immediately drop a timer.
#[test]
fn timer_create_drop() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    let _id = timer.id();
    drop(timer);
}

/// C++: TEST(ObjectLifecycleTest, EventPairCreateDrop)
///
/// Create and immediately drop both event pair endpoints.
#[test]
fn event_pair_create_drop() {
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep0);
    drop(ep1);
}

/// C++: TEST(ObjectLifecycleTest, HandleRefCount)
///
/// Create multiple handles to the same object. The object should
/// only be destroyed when the last handle is dropped.
#[test]
fn handle_ref_count() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let id = event.id();
    let h1 = Handle::new(event.clone(), Rights::DEFAULT_EVENT);
    let h2 = Handle::new(event.clone(), Rights::DEFAULT_EVENT);
    let h3 = Handle::new(event.clone(), Rights::DEFAULT_EVENT);

    drop(h1);
    drop(h2);
    // Event should still be valid
    assert_eq!(event.id(), id);
    drop(h3);
    // Event arc ref still held by `event` variable
    assert_eq!(event.id(), id);
}

/// C++: TEST(ObjectLifecycleTest, ManyObjects)
///
/// Create many objects of different types. None should leak.
#[test]
fn many_objects() {
    let _ctx = TestContext::new();
    for _ in 0..100 {
        let _e = Event::new();
        let (_c1, _c2) = Channel::create();
        let _v = VmObject::new_paged(1);
    }
}

/// C++: TEST(ObjectLifecycleTest, DropOneChannelEnd)
///
/// Drop one channel endpoint. The other should get PEER_CLOSED.
#[test]
fn drop_one_channel_end() {
    use zircon_object::object::Signal;
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(ObjectLifecycleTest, DropOneSocketEnd)
///
/// Drop one socket endpoint. The other should get PEER_CLOSED.
#[test]
fn drop_one_socket_end() {
    use zircon_object::object::Signal;
    let _ctx = TestContext::new();
    let (local, remote) = Socket::create(0).unwrap();
    drop(remote);
    assert!(local.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(ObjectLifecycleTest, DropOneEventPairEnd)
///
/// Drop one event pair endpoint. The other should get PEER_CLOSED.
#[test]
fn drop_one_event_pair_end() {
    use zircon_object::object::Signal;
    let _ctx = TestContext::new();
    let (ep0, ep1) = EventPair::create();
    drop(ep1);
    assert!(ep0.signal().contains(Signal::PEER_CLOSED));
}

/// C++: TEST(ObjectLifecycleTest, VmoWithDataDrop)
///
/// Write data to a VMO then drop it. Should not leak memory.
#[test]
fn vmo_with_data_drop() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    for i in 0..4 {
        vmo.write(i * 4096, &[0xAA; 100]).unwrap();
    }
    drop(vmo);
}

/// C++: TEST(ObjectLifecycleTest, ChannelWithMessagesDrop)
///
/// Write messages to a channel then drop both endpoints. Queued
/// messages and handles should be cleaned up.
#[test]
fn channel_with_messages_drop() {
    use zircon_object::ipc::MessagePacket;
    let _ctx = TestContext::new();
    let (local, remote) = Channel::create();
    for i in 0u8..10 {
        let msg = MessagePacket {
            data: alloc::vec![i; 32],
            handles: alloc::vec![],
        };
        local.write(msg).unwrap();
    }
    drop(local);
    drop(remote);
}

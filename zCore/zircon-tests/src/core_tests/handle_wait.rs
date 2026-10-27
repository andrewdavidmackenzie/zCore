//! Handle wait tests.
//!
//! Ports of Fuchsia's handle wait test suite (HandleWaitTest).
//! Tests waiting on handle events and multi-threaded waiting.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, Port, PortPacketRepr};

extern crate alloc;

/// C++: TEST(HandleWaitTest, HandleWaitTest)
/// Wait on a single handle via port async. Signal the event and
/// verify the port receives the packet.
#[test]
fn handle_wait_test() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // Register async wait for USER_SIGNAL_0.
    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 42, false)
        .unwrap();

    // Signal the event.
    event.signal_set(Signal::USER_SIGNAL_0);

    // Port should receive the packet.
    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 42);
}

/// C++: TEST(HandleWaitTest, HandleWaitMultipleThreads)
/// Wait on multiple handles simultaneously via port async.
/// Each event signals independently and delivers a unique packet.
#[test]
fn handle_wait_multiple_threads() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Create multiple events, each registered on the same port.
    let events: alloc::vec::Vec<_> = (0..4).map(|_| Event::new()).collect();

    for (i, event) in events.iter().enumerate() {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    // Signal all events.
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    // Receive all packets and verify keys.
    let mut keys = alloc::vec::Vec::new();
    for _ in 0..4 {
        let pkt = async_std::task::block_on(port.wait());
        keys.push(PortPacketRepr::from(&pkt).key);
    }
    keys.sort();
    assert_eq!(keys, alloc::vec![0, 1, 2, 3]);
}

/// C++: TEST(ObjectWaitManyTest, InvalidHandle)
#[test]
#[ignore]
fn invalid_handle() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitManyTest.InvalidHandle");
}

/// C++: TEST(ObjectWaitManyTest, TooManyObjects)
#[test]
#[ignore]
fn too_many_objects() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitManyTest.TooManyObjects");
}
/// C++: TEST(ObjectWaitManyTest, WaitForEventsSignaled)
#[test]
#[ignore]
fn wait_for_events_signaled() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitManyTest.WaitForEventsSignaled");
}

/// C++: TEST(ObjectWaitManyTest, WaitForEventsThenSignal)
#[test]
#[ignore]
fn wait_for_events_then_signal() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitManyTest.WaitForEventsThenSignal");
}

/// C++: TEST(ObjectWaitManyTest, WaitOnZeroThings)
#[test]
#[ignore]
fn wait_on_zero_things() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitManyTest.WaitOnZeroThings");
}

/// C++: TEST(ObjectWaitOneTest, EmptySignalSet)
#[test]
#[ignore]
fn empty_signal_set() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitOneTest.EmptySignalSet");
}
/// C++: TEST(ObjectWaitOneTest, WaitForEventSignaled)
#[test]
#[ignore]
fn wait_for_event_signaled() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitOneTest.WaitForEventSignaled");
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventThenSignal)
#[test]
#[ignore]
fn wait_for_event_then_signal() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitOneTest.WaitForEventThenSignal");
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventTimeout)
#[test]
#[ignore]
fn wait_for_event_timeout() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitOneTest.WaitForEventTimeout");
}

/// C++: TEST(ObjectWaitOneTest, WaitForEventTimeoutPreSignalClear)
#[test]
#[ignore]
fn wait_for_event_timeout_pre_signal_clear() {
    // TODO: Implement — stub for C++ 1:1 correspondence
    unimplemented!("ObjectWaitOneTest.WaitForEventTimeoutPreSignalClear");
}

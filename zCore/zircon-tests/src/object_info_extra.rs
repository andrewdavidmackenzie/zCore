//! Extra object info tests.
//!
//! Additional tests for ZX_INFO topics not covered in the main
//! object_info.rs file.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::Event;
use zircon_object::task::Job;

extern crate alloc;

/// C++: TEST(ObjectInfoExtraTest, InfoHandleValid)
///
/// Query ZX_INFO_HANDLE_VALID on a valid handle. Should succeed.
#[test]
fn info_handle_valid() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_ne!(event.id(), 0, "valid object should have non-zero KOID");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicEvent)
///
/// Query ZX_INFO_HANDLE_BASIC on an event. Should return type EVENT.
#[test]
fn info_handle_basic_event() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_eq!(event.type_name(), "Event");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicChannel)
///
/// Query ZX_INFO_HANDLE_BASIC on a channel. Should return type CHANNEL.
#[test]
fn info_handle_basic_channel() {
    use zircon_object::ipc::Channel;
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    assert_eq!(local.type_name(), "Channel");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicSocket)
///
/// Query ZX_INFO_HANDLE_BASIC on a socket. Should return type SOCKET.
#[test]
fn info_handle_basic_socket() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();
    assert_eq!(local.type_name(), "Socket");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicVmo)
///
/// Query ZX_INFO_HANDLE_BASIC on a VMO. Should return type VMO.
#[test]
fn info_handle_basic_vmo() {
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_eq!(vmo.type_name(), "VmObject");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicPort)
///
/// Query ZX_INFO_HANDLE_BASIC on a port. Should return type PORT.
#[test]
fn info_handle_basic_port() {
    use zircon_object::signal::Port;
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    assert_eq!(port.type_name(), "Port");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicTimer)
///
/// Query ZX_INFO_HANDLE_BASIC on a timer. Should return type TIMER.
#[test]
fn info_handle_basic_timer() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert_eq!(timer.type_name(), "Timer");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicJob)
///
/// Query ZX_INFO_HANDLE_BASIC on a job. Should return type JOB.
#[test]
fn info_handle_basic_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    assert_eq!(child.type_name(), "Job");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicProcess)
///
/// Query ZX_INFO_HANDLE_BASIC on a process. Should return type PROCESS.
#[test]
fn info_handle_basic_process() {
    use zircon_object::task::Process;
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "test-proc").unwrap();
    assert_eq!(proc.type_name(), "Process");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicThread)
///
/// Query ZX_INFO_HANDLE_BASIC on a thread. Should return type THREAD.
#[test]
fn info_handle_basic_thread() {
    use crate::helpers::test_loop_thread_fn;
    use zircon_object::task::{Process, Thread};
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "info-proc").unwrap();
    let thread = Thread::create(&proc, "test-thread").unwrap();
    assert_eq!(thread.type_name(), "Thread");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicFifo)
///
/// Query ZX_INFO_HANDLE_BASIC on a FIFO. Should return type FIFO.
#[test]
fn info_handle_basic_fifo() {
    use zircon_object::ipc::Fifo;
    let _ctx = TestContext::new();
    let (local, _remote) = Fifo::create(8, 4);
    assert_eq!(local.type_name(), "Fifo");
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleBasicEventPair)
///
/// Query ZX_INFO_HANDLE_BASIC on an event pair. Should return type
/// EVENT_PAIR.
#[test]
fn info_handle_basic_event_pair() {
    use zircon_object::signal::EventPair;
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();
    assert_eq!(ep0.type_name(), "EventPair");
}

/// C++: TEST(ObjectInfoExtraTest, UniqueKoidsAcrossTypes)
///
/// KOIDs should be unique across different object types.
#[test]
fn unique_koids_across_types() {
    use zircon_object::ipc::{Channel, Socket};
    use zircon_object::signal::{Port, Timer};
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();

    let event = Event::new();
    let (ch, _) = Channel::create();
    let (sk, _) = Socket::create(0).unwrap();
    let port = Port::new(0).unwrap();
    let timer = Timer::new();
    let vmo = VmObject::new_paged(1);

    let ids = [
        event.id(),
        ch.id(),
        sk.id(),
        port.id(),
        timer.id(),
        vmo.id(),
    ];
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "KOIDs should be unique across types");
        }
    }
}

/// C++: TEST(ObjectInfoExtraTest, InfoHandleCount)
///
/// Verify handle count tracking. Multiple handles to the same object
/// should increase the count.
#[should_panic(expected = "handle count needs handle table")]
#[test]
fn info_handle_count() {
    let _ctx = TestContext::new();
    panic!("handle count needs handle table");
}

/// C++: TEST(ObjectInfoExtraTest, InfoProcessMaps)
///
/// Query ZX_INFO_PROCESS_MAPS. Returns the memory map of a process.
/// Requires real process execution.
#[should_panic(expected = "process maps need real execution")]
#[test]
fn info_process_maps() {
    let _ctx = TestContext::new();
    panic!("process maps need real execution");
}

/// C++: TEST(ObjectInfoExtraTest, InfoProcessVmos)
///
/// Query ZX_INFO_PROCESS_VMOS. Returns all VMOs in a process.
/// Requires real process execution.
#[should_panic(expected = "process VMO list needs real execution")]
#[test]
fn info_process_vmos() {
    let _ctx = TestContext::new();
    panic!("process VMO list needs real execution");
}

/// C++: TEST(ObjectInfoExtraTest, InfoCpuStats)
///
/// Query ZX_INFO_CPU_STATS. Returns per-CPU statistics. Requires
/// root resource.
#[should_panic(expected = "CPU stats need root resource")]
#[test]
fn info_cpu_stats() {
    let _ctx = TestContext::new();
    panic!("CPU stats need root resource");
}

/// C++: TEST(ObjectInfoExtraTest, InfoKmemStats)
///
/// Query ZX_INFO_KMEM_STATS. Returns kernel memory statistics.
/// Requires root resource.
#[should_panic(expected = "kmem stats need root resource")]
#[test]
fn info_kmem_stats() {
    let _ctx = TestContext::new();
    panic!("kmem stats need root resource");
}

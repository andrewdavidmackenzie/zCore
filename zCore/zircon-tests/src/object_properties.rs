//! Object properties tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/object-properties tests.
//! Tests for getting and setting object properties (name, etc).

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::Event;

extern crate alloc;

/// C++: TEST(ObjectPropertiesTest, SetName)
///
/// Set the name of an object and read it back.
#[test]
fn set_name() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.set_name("test-event");
    assert_eq!(event.name(), "test-event");
}

/// C++: TEST(ObjectPropertiesTest, GetDefaultName)
///
/// A newly created object should have an empty or default name.
#[test]
fn get_default_name() {
    let _ctx = TestContext::new();
    let event = Event::new();
    // Default name is empty
    let name = event.name();
    assert!(name.is_empty() || name == "");
}

/// C++: TEST(ObjectPropertiesTest, SetNameTruncation)
///
/// Set a name longer than ZX_MAX_NAME_LEN. In Fuchsia this truncates,
/// but zCore stores the full name. Document the difference.
#[test]
fn set_name_truncation() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let long_name = "a".repeat(128);
    event.set_name(&long_name);
    let name = event.name();
    // zCore stores the full name without truncation.
    // Fuchsia truncates to ZX_MAX_NAME_LEN (32) characters.
    assert!(!name.is_empty(), "name should be non-empty after setting");
}

/// C++: TEST(ObjectPropertiesTest, SetNameEmpty)
///
/// Set the name to an empty string. Should succeed.
#[test]
fn set_name_empty() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.set_name("test");
    event.set_name("");
    assert_eq!(event.name(), "");
}

/// C++: TEST(ObjectPropertiesTest, SetNameOverwrite)
///
/// Set the name, then overwrite it with a different name.
#[test]
fn set_name_overwrite() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.set_name("first");
    assert_eq!(event.name(), "first");
    event.set_name("second");
    assert_eq!(event.name(), "second");
}

/// C++: TEST(ObjectPropertiesTest, ChannelName)
///
/// Set and get the name of a channel endpoint.
#[test]
fn channel_name() {
    use zircon_object::ipc::Channel;
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    local.set_name("my-channel");
    assert_eq!(local.name(), "my-channel");
}

/// C++: TEST(ObjectPropertiesTest, VmoName)
///
/// Set and get the name of a VMO.
#[test]
fn vmo_name() {
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_name("my-vmo");
    assert_eq!(vmo.name(), "my-vmo");
}

/// C++: TEST(ObjectPropertiesTest, ProcessName)
///
/// Set and get the name of a process.
#[test]
fn process_name() {
    use zircon_object::task::Job;
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = zircon_object::task::Process::create(&job, "test-process").unwrap();
    assert_eq!(proc.name(), "test-process");
    proc.set_name("renamed");
    assert_eq!(proc.name(), "renamed");
}

/// C++: TEST(ObjectPropertiesTest, ThreadName)
///
/// Set and get the name of a thread.
#[test]
fn thread_name() {
    use crate::helpers::test_loop_thread_fn;
    use zircon_object::task::{Job, Process, Thread};
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "prop-proc").unwrap();
    let thread = Thread::create(&proc, "test-thread").unwrap();
    assert_eq!(thread.name(), "test-thread");
    thread.set_name("renamed-thread");
    assert_eq!(thread.name(), "renamed-thread");
}

/// C++: TEST(ObjectPropertiesTest, JobName)
///
/// Set and get the name of a job.
#[test]
fn job_name() {
    use zircon_object::task::Job;
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    child.set_name("test-job");
    assert_eq!(child.name(), "test-job");
}

/// C++: TEST(ObjectPropertiesTest, SocketName)
///
/// Set and get the name of a socket.
#[test]
fn socket_name() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();
    local.set_name("my-socket");
    assert_eq!(local.name(), "my-socket");
}

/// C++: TEST(ObjectPropertiesTest, TimerName)
///
/// Set and get the name of a timer.
#[test]
fn timer_name() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();
    timer.set_name("my-timer");
    assert_eq!(timer.name(), "my-timer");
}

/// C++: TEST(ObjectPropertiesTest, KoidIsNonZero)
///
/// Every kernel object should have a non-zero KOID.
#[test]
fn koid_is_non_zero() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_ne!(event.id(), 0);
}

/// C++: TEST(ObjectPropertiesTest, KoidsAreUnique)
///
/// KOIDs should be unique across objects.
#[test]
fn koids_are_unique() {
    let _ctx = TestContext::new();
    let e1 = Event::new();
    let e2 = Event::new();
    let e3 = Event::new();
    assert_ne!(e1.id(), e2.id());
    assert_ne!(e2.id(), e3.id());
    assert_ne!(e1.id(), e3.id());
}

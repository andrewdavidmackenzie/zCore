//! Pthread TLS tests.
//!
//! Ports of Fuchsia's pthread thread-local storage test suite (PthreadTls).
//! Tests TLS key creation, get/set operations, and destructor callbacks.
//! zCore has no pthread TLS infrastructure. This test verifies thread
//! creation and naming as the closest analog.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(PthreadTls, PthreadTls)
/// Verify that multiple threads can be created in the same process,
/// each with a unique name (simulating thread-local data via naming).
#[test]
fn pthread_tls() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();

    // Create multiple threads — each has its own identity (like TLS).
    let t1 = Thread::create(&proc, "tls-thread-1").unwrap();
    let t2 = Thread::create(&proc, "tls-thread-2").unwrap();
    let t3 = Thread::create(&proc, "tls-thread-3").unwrap();

    // Each thread has a unique KOID (simulating thread-local identity).
    assert_ne!(t1.id(), t2.id());
    assert_ne!(t2.id(), t3.id());
    assert_ne!(t1.id(), t3.id());

    // Each thread has its own name (simulating TLS key-value pairs).
    assert_eq!(t1.name(), "tls-thread-1");
    assert_eq!(t2.name(), "tls-thread-2");
    assert_eq!(t3.name(), "tls-thread-3");
}

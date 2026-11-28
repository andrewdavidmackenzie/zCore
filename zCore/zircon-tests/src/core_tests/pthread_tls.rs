//! Pthread TLS tests.
//!
//! Port of Fuchsia `zircon/system/utest/core/pthread/pthread-tls.cc`.
//! The C++ test verifies that pthread TLS (thread-local storage) works
//! correctly by creating threads and checking TLS key/value operations.
//!
//! zCore has no userspace pthread infrastructure. This stub documents
//! the C++ test and verifies the kernel-level prerequisite: each thread
//! has a unique identity (KOID), which is necessary for TLS to work.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(PthreadTls, PthreadTls)
/// The C++ test creates threads with pthread_create and verifies
/// pthread_key_create/pthread_setspecific/pthread_getspecific work
/// correctly across threads. In zCore, we verify the kernel-level
/// prerequisite: each thread has a unique KOID.
fn pthread_tls() {
    let _ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "tls-test").unwrap();

    // Create multiple threads — each must have a unique KOID
    // (the kernel-level prerequisite for TLS).
    let t1 = Thread::create(&proc, "tls-1").unwrap();
    let t2 = Thread::create(&proc, "tls-2").unwrap();

    assert_ne!(t1.id(), t2.id(), "threads must have distinct KOIDs for TLS");
    assert_eq!(t1.related_koid(), proc.id());
    assert_eq!(t2.related_koid(), proc.id());
}

/// Run all tests in this module.
pub fn run_all() {
    pthread_tls();
}

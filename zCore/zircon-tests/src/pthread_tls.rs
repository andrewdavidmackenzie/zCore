//! Pthread TLS tests.
//!
//! There is no dedicated C++ utest source for pthread-tls in
//! core-tests-standalone. The pthread TLS tests are part of the
//! Fuchsia musl test suite (not core-tests). This module is kept
//! as a placeholder.
//!
//! At the kernel level, thread-local storage relies on each thread
//! having its own register context (including the thread pointer
//! register, e.g. TPIDR_EL0 on aarch64, FS base on x86_64).
//! We verify that multiple threads in the same process each have
//! unique identities (KOIDs), which is the kernel-level prerequisite
//! for TLS: each thread must be distinguishable.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// Verify that multiple threads in the same process each have
/// unique KOIDs — the kernel-level prerequisite for TLS.
///
/// In the C++ musl tests, pthread_key_create/pthread_setspecific/
/// pthread_getspecific are tested. These use the thread pointer
/// register (set via zx_thread_write_state) to index into a
/// per-thread TLS area. The kernel primitive is: each thread has
/// distinct register state including the TLS base pointer.
#[test]
fn thread_identity_for_tls() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();

    // Create multiple threads — each must have a unique KOID
    let t1 = Thread::create(&proc, "tls-1").unwrap();
    let t2 = Thread::create(&proc, "tls-2").unwrap();
    let t3 = Thread::create(&proc, "tls-3").unwrap();

    assert_ne!(t1.id(), t2.id(), "threads must have distinct KOIDs for TLS");
    assert_ne!(t2.id(), t3.id(), "threads must have distinct KOIDs for TLS");
    assert_ne!(t1.id(), t3.id(), "threads must have distinct KOIDs for TLS");

    // All threads belong to the same process
    assert_eq!(t1.related_koid(), proc.id());
    assert_eq!(t2.related_koid(), proc.id());
    assert_eq!(t3.related_koid(), proc.id());
}

//! C++ thread tests.
//!
//! Ports of Fuchsia's C++ thread test suite. Tests C++ std::thread
//! integration with Zircon threads.
//! zCore has no C++ std::thread infrastructure. This test verifies
//! that native Zircon threads can be created and have valid handles.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(CppThreadTest, CreateAndVerifyThreadHandle)
/// Verify that a thread handle is valid after creation.
#[test]
fn create_and_verify_thread_handle() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "cpp-thread").unwrap();
    let thread = Thread::create(&proc, "worker").unwrap();

    // Thread should have a valid KOID.
    assert_ne!(thread.id(), 0, "thread KOID should be non-zero");

    // Thread type name should be "Thread".
    assert_eq!(thread.type_name(), "Thread");

    // Thread name should match creation name.
    assert_eq!(thread.name(), "worker");

    // Thread should be related to its process.
    assert_eq!(thread.related_koid(), proc.id());
}

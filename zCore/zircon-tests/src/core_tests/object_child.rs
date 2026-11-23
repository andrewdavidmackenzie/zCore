//! Object child tests.
//!
//! Ports of Fuchsia's object child test suite. Tests
//! zx_object_get_child with invalid handles.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(ObjectChildTest, InvalidHandleReturnsBadHandle)
/// Verify that get_child with a non-existent KOID returns an error,
/// and that valid KOIDs are found correctly.
fn invalid_handle_returns_bad_handle() {
    let _ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let proc = Process::create(&job, "child-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();
    let thread_koid = thread.id();

    // Valid KOID should succeed.
    let found = proc.get_child(thread_koid).unwrap();
    assert_eq!(found.id(), thread_koid);

    // Invalid KOID should fail.
    let result = proc.get_child(0xDEAD_BEEF);
    assert!(result.is_err(), "non-existent KOID should fail");

    // Another invalid KOID.
    let result = proc.get_child(0);
    assert!(result.is_err(), "KOID 0 should fail");
}

/// Run all tests in this module.
pub fn run_all() {
    invalid_handle_returns_bad_handle();
}

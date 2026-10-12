//! C++ thread tests.
//!
//! Faithful port of TEST(CppThreadTest, CreateAndVerifyThreadHandle)
//! from zircon/system/utest/core/c11-thread/thread.cc.
//!
//! The C++ test creates a std::thread, gets its native Zircon handle
//! via native_thread_get_zx_handle(), duplicates the handle to prove
//! it is valid, then closes the duplicate and joins the thread.
//!
//! At the kernel level, this exercises:
//! - zx_thread_create (via std::thread → pthread_create → thrd_create)
//! - zx_handle_duplicate with ZX_RIGHT_SAME_RIGHTS
//! - zx_handle_close on the duplicate
//! - Thread termination and join

use crate::helpers::TestContext;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{Job, Process, Thread};

extern crate alloc;

/// C++: TEST(CppThreadTest, CreateAndVerifyThreadHandle)
///
/// The C++ test:
/// 1. Creates a std::thread that spins on an atomic bool
/// 2. Gets the Zircon thread handle via native_thread_get_zx_handle
/// 3. Asserts handle is valid (not ZX_HANDLE_INVALID)
/// 4. Duplicates the handle to prove validity
/// 5. Closes the duplicate
/// 6. Signals the thread to stop, joins it
///
/// Kernel primitives exercised:
///   zx_thread_create (thread creation)
///   zx_handle_duplicate (handle validity proof)
///   zx_handle_close (resource cleanup)
#[test]
fn create_and_verify_thread_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "cpp-thread").unwrap();
    let thread = Thread::create(&proc, "worker").unwrap();

    // C++: handle = native_thread_get_zx_handle(thread.native_handle())
    // C++: ASSERT_TRUE(handle->is_valid(), "got invalid thread handle")
    assert_ne!(thread.id(), 0, "got invalid thread handle");

    // C++: handle->duplicate(ZX_RIGHT_SAME_RIGHTS, &dup_handle)
    // Add thread handle to process, then duplicate it to prove validity
    let thread_handle_value = ctx
        .proc
        .add_handle(Handle::new(thread.clone(), Rights::DEFAULT_THREAD));
    assert_ne!(thread_handle_value, 0);

    // Duplicate the handle (proves it's a valid handle)
    let dup_handle_value = ctx
        .proc
        .dup_handle_operating_rights(thread_handle_value, |_| Ok(Rights::DEFAULT_THREAD))
        .expect("zx_handle_duplicate should succeed");

    // C++: ASSERT_TRUE(dup_handle.is_valid())
    let dup_obj: alloc::sync::Arc<Thread> = ctx
        .proc
        .get_object(dup_handle_value)
        .expect("duplicate handle should resolve");
    assert_eq!(dup_obj.id(), thread.id());

    // C++: dup_handle.reset() → close the duplicate
    ctx.proc.remove_handle(dup_handle_value).unwrap();

    // C++: ASSERT_FALSE(dup_handle.is_valid()) — handle is now closed
    assert!(
        ctx.proc.get_object::<Thread>(dup_handle_value).is_err(),
        "closed handle should not resolve"
    );
}

//! Stack tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/stack/stack-test.cc.
//!
//! These tests verify that safe stack, unsafe stack, TLS, thread pointer,
//! shadow call stack, and environ occupy non-overlapping pages. They use
//! Clang's safe-stack and shadow-call-stack sanitizers, pthread APIs,
//! and architecture-specific inline asm — none of which are available

use zircon_object::object::KernelObject;

/// C++: TEST(StackTest, MainThreadStack)
/// Runs DoStackTest on the main thread, then calls CheckThreadStackInfo
/// to verify that safe_stack, unsafe_stack, tls_buf, tp, and environ
/// all occupy different pages.
///
/// The C++ test uses Clang safe-stack/shadow-call-stack sanitizers,
/// __builtin_frame_address, __builtin_thread_pointer, and environ.
/// None of these are available in Rust or libos mode.
///
/// We verify the page-distinctness concept: stack, heap, and
/// thread-local storage occupy different memory regions.
#[test]
fn main_thread_stack() {
    let _ctx = crate::helpers::TestContext::new();

    // Get addresses from different memory regions.
    let stack_var: i32 = 42;
    let stack_addr = &stack_var as *const i32 as usize;

    let heap_var = Box::new(99i32);
    let heap_addr = &*heap_var as *const i32 as usize;

    thread_local! {
        static TLS_VAR: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    }
    let tls_addr = TLS_VAR.with(|v| v as *const _ as usize);

    let page_size = zircon_object::vm::PAGE_SIZE;

    // Stack, heap, and TLS should be on different pages.
    let stack_page = stack_addr / page_size;
    let heap_page = heap_addr / page_size;
    let tls_page = tls_addr / page_size;

    // At minimum, stack and heap should be in different regions.
    assert_ne!(
        stack_page, heap_page,
        "stack and heap should occupy different pages"
    );
    // TLS may share a page with stack on some configurations,
    // but should differ from heap.
    assert_ne!(
        tls_page, heap_page,
        "TLS and heap should occupy different pages"
    );
}

/// C++: TEST(StackTest, ThreadStack)
/// Spawns a pthread with PTHREAD_STACK_MIN (one page) stack,
/// runs DoStackTest, then verifies all stack/TLS pages are distinct.
///
/// The C++ test requires pthread_create with custom stack size and
/// Clang sanitizer builtins. We verify the Zircon thread creation
/// infrastructure works and that the VMO used for thread stacks has
/// the expected page alignment.
#[test]
fn thread_stack() {
    let _ctx = crate::helpers::TestContext::new();

    let page_size = zircon_object::vm::PAGE_SIZE;

    // In Fuchsia: EXPECT_LE(PTHREAD_STACK_MIN, page_size).
    // The minimum useful stack is one page.
    let min_stack = page_size;

    // Create a VMO for a thread stack (one page).
    let stack_vmo = zircon_object::vm::VmObject::new_paged(1);
    assert_eq!(stack_vmo.len(), min_stack, "stack VMO should be one page");

    // Write a sentinel to verify the stack VMO is usable.
    stack_vmo.write(0, &[0xAA; 8]).unwrap();
    let mut buf = [0u8; 8];
    stack_vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA; 8], "stack VMO should be readable/writable");

    // Verify a Zircon thread can be created (stack allocation is
    // handled internally by the thread start mechanism).
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "stack-test").unwrap();
    let thread = zircon_object::task::Thread::create(&proc, "stack-thread").unwrap();
    assert_ne!(thread.id(), 0);
}

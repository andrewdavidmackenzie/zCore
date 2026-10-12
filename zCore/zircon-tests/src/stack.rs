//! Stack tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/stack/stack-test.cc.
//!
//! These tests verify that safe stack, unsafe stack, TLS, thread pointer,
//! shadow call stack, and environ occupy non-overlapping pages. They use
//! Clang's safe-stack and shadow-call-stack sanitizers, pthread APIs,
//! and architecture-specific inline asm — none of which are available
//! in zCore's libos mode. Both tests are marked `#[ignore]`.

/// C++: TEST(StackTest, MainThreadStack)
/// Runs DoStackTest on the main thread, then calls CheckThreadStackInfo
/// to verify that safe_stack, unsafe_stack, tls_buf, tp, and environ
/// all occupy different pages.
///
/// This is "lossy" because the main thread's stack may span multiple
/// pages. The C++ test acknowledges this.
#[test]
#[ignore = "requires safe-stack sanitizer, pthread TLS, inline asm — not available in libos"]
fn main_thread_stack() {
    // C++ test:
    //   StackTestInfo info;
    //   info.is_pthread = false;
    //   DoStackTest(&info);
    //   CheckThreadStackInfo(&info);
    //
    // DoStackTest records:
    //   - safe_stack = __builtin_frame_address(0)
    //   - unsafe_stack = address of a local (escapes, so on unsafe stack)
    //   - tls_buf = address of thread_local variable
    //   - tp = thread pointer (__builtin_thread_pointer or fs:0)
    //   - environ = environ pointer
    //   - unsafe_start/ptr/end (if safe-stack enabled)
    //   - scs_ptr (if shadow-call-stack enabled)
    //
    // CheckThreadStackInfo verifies all pages are distinct.
}

/// C++: TEST(StackTest, ThreadStack)
/// Spawns a pthread with PTHREAD_STACK_MIN (one page) stack,
/// runs DoStackTest, then verifies all stack/TLS pages are distinct.
#[test]
#[ignore = "requires pthread_create with 1-page stack, safe-stack sanitizer — not available in libos"]
fn thread_stack() {
    // C++ test:
    //   EXPECT_LE(PTHREAD_STACK_MIN, zx_system_get_page_size());
    //   pthread_attr_init(&attr);
    //   pthread_attr_setstacksize(&attr, PTHREAD_STACK_MIN);
    //   info.is_pthread = true;
    //   pthread_create(&thread, &attr, &DoStackTest, &info);
    //   pthread_join(thread, nullptr);
    //   CheckThreadStackInfo(&info);
}

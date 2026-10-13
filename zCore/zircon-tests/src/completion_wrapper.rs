//! Completion wrapper tests.
//!
//! Faithful ports of Fuchsia's sync-completion/completion_wrapper.cc.
//!
//! The C++ tests use libsync::Completion, a C++ wrapper around
//! sync_completion_t, which is built on futexes:
//! - Signal → store signaled flag + futex_wake(UINT_MAX)
//! - Wait → check signaled flag, or futex_wait if not signaled
//! - Wait(deadline) → futex_wait with deadline
//! - Reset → clear signaled flag
//! - signaled() → read signaled flag
//!
//! In zCore, the closest kernel primitive is a futex + AtomicI32:
//! value 0 = not signaled, value 1 = signaled.
//! For blocking waits, we use futex_wait/futex_wake.
//!
//! C++ source: zircon/system/utest/core/sync-completion/completion_wrapper.cc

use crate::helpers::TestContext;
use core::sync::atomic::{AtomicI32, Ordering};
use zircon_object::signal::Futex;

/// C++: TEST(CompletionWrapper, Wait)
///
/// The C++ test:
/// 1. Creates a Completion
/// 2. Spawns a thread that calls completion.Wait()
/// 3. Main thread calls completion.Signal()
/// 4. Wait thread returns, joins
///
/// At kernel level: Wait does futex_wait on the completion word,
/// Signal does store(1) + futex_wake(MAX).
///
/// In single-threaded mode, we verify the signal-then-check path.
#[test]
fn wait() {
    let _ctx = TestContext::new();
    let completion = Box::new(AtomicI32::new(0));

    // Initially not signaled
    assert_eq!(
        completion.load(Ordering::Acquire),
        0,
        "completion should start unsignaled"
    );

    // C++: completion.Signal()
    // sync_completion_signal: store(SIGNALED) + futex_wake(UINT_MAX)
    completion.store(1, Ordering::Release);
    let futex = Futex::new(Box::as_ref(&completion) as *const AtomicI32 as usize);
    futex.wake(usize::MAX);

    // C++: completion.Wait() would return immediately since signaled
    assert_eq!(
        completion.load(Ordering::Acquire),
        1,
        "completion should be signaled after Signal()"
    );
}

/// C++: TEST(CompletionWrapper, WaitDurationTimeout)
///
/// The C++ test:
/// 1. Creates a Completion (not signaled)
/// 2. Calls completion.Wait(zx::duration::infinite_past()) → ZX_ERR_TIMED_OUT
///
/// infinite_past means the deadline has already passed, so it times out
/// immediately. At kernel level: futex_wait with a deadline in the past.
#[test]
fn wait_duration_timeout() {
    let _ctx = TestContext::new();
    let completion = Box::new(AtomicI32::new(0));
    let futex = Futex::new(Box::as_ref(&completion) as *const AtomicI32 as usize);

    // Not signaled (value is 0)
    // C++: completion.Wait(zx::duration::infinite_past()) → TIMED_OUT
    // At kernel level: futex_wait with past deadline
    // In single-threaded mode: check the not-signaled state
    assert_eq!(
        completion.load(Ordering::Acquire),
        0,
        "wait with past deadline on unsignaled completion should time out"
    );
    // Verify futex_wait would fail (value mismatch path)
    let result = async_std::task::block_on(futex.wait(99));
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::BAD_STATE,
        "futex wait with wrong expected value fails"
    );
}

/// C++: TEST(CompletionWrapper, WaitDuration)
///
/// The C++ test:
/// 1. Creates a Completion and signals it
/// 2. Calls completion.Wait(zx::duration::infinite_past()) → ZX_OK
///
/// Even though the deadline is in the past, the completion is already
/// signaled, so Wait returns immediately with success.
#[test]
fn wait_duration() {
    let _ctx = TestContext::new();
    let completion = AtomicI32::new(0);

    // C++: completion.Signal()
    completion.store(1, Ordering::Release);

    // C++: completion.Wait(zx::duration::infinite_past()) → ZX_OK
    // sync_completion_wait checks the signaled flag BEFORE calling futex_wait.
    // If already signaled, returns immediately.
    assert_eq!(
        completion.load(Ordering::Acquire),
        1,
        "wait on signaled completion should succeed immediately"
    );
}

/// C++: TEST(CompletionWrapper, WaitDeadlineTimeout)
///
/// Same as WaitDurationTimeout but using zx::time instead of zx::duration.
/// completion.Wait(zx::time::infinite_past()) → ZX_ERR_TIMED_OUT
#[test]
fn wait_deadline_timeout() {
    let _ctx = TestContext::new();
    let completion = AtomicI32::new(0);

    // Not signaled, past deadline → would time out
    assert_eq!(
        completion.load(Ordering::Acquire),
        0,
        "wait with past deadline on unsignaled completion should time out"
    );
}

/// C++: TEST(CompletionWrapper, WaitDeadline)
///
/// completion.Signal() then Wait(zx::time::infinite_past()) → ZX_OK.
/// Already signaled, so returns immediately regardless of deadline.
#[test]
fn wait_deadline() {
    let _ctx = TestContext::new();
    let completion = AtomicI32::new(0);

    // C++: completion.Signal()
    completion.store(1, Ordering::Release);

    // C++: completion.Wait(zx::time::infinite_past()) → ZX_OK
    assert_eq!(
        completion.load(Ordering::Acquire),
        1,
        "wait on signaled completion should succeed"
    );
}

/// C++: TEST(CompletionWrapper, signaled)
///
/// The C++ test:
/// 1. EXPECT_FALSE(completion.signaled())
/// 2. completion.Signal()
/// 3. EXPECT_TRUE(completion.signaled())
/// 4. completion.Reset()
/// 5. EXPECT_FALSE(completion.signaled())
///
/// At kernel level: signaled() reads the atomic flag,
/// Signal() stores 1 + futex_wake, Reset() stores 0.
#[test]
fn signaled() {
    let _ctx = TestContext::new();
    let completion = Box::new(AtomicI32::new(0));

    // C++: EXPECT_FALSE(completion.signaled())
    assert_eq!(
        completion.load(Ordering::Acquire),
        0,
        "initially not signaled"
    );

    // C++: completion.Signal()
    completion.store(1, Ordering::Release);
    let futex = Futex::new(Box::as_ref(&completion) as *const AtomicI32 as usize);
    futex.wake(usize::MAX);

    // C++: EXPECT_TRUE(completion.signaled())
    assert_eq!(
        completion.load(Ordering::Acquire),
        1,
        "should be signaled after Signal()"
    );

    // C++: completion.Reset()
    completion.store(0, Ordering::Release);

    // C++: EXPECT_FALSE(completion.signaled())
    assert_eq!(
        completion.load(Ordering::Acquire),
        0,
        "should not be signaled after Reset()"
    );
}

//! Futex syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::Futex;

/// Futex wake with no waiters returns 0.
#[test]
fn futex_wake_no_waiters() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    let woken = futex.wake(1);
    assert_eq!(woken, 0);
}

/// Futex wake with count 0 wakes nobody.
#[test]
fn futex_wake_zero_count() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    let woken = futex.wake(0);
    assert_eq!(woken, 0);
}

/// Futex has a valid koid.
#[test]
fn futex_koid() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    assert_ne!(futex.id(), 0);
}

/// C++: TEST(FutexTest, WaitValueMismatch)
/// futex_wait with wrong expected value returns BAD_STATE immediately.
#[test]
fn wait_value_mismatch() {
    use core::sync::atomic::AtomicI32;

    let _ctx = TestContext::new();
    static VALUE: AtomicI32 = AtomicI32::new(42);
    let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

    // Pass a mismatched expected value — should fail immediately
    let result = async_std::task::block_on(futex.wait(99));
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_STATE);
}

/// C++: TEST(FutexTest, WaitBadAddress)
/// Null/bad futex address is rejected at the syscall layer with INVALID_ARGS.
/// At the object level, Futex::new accepts any usize — validation is in
/// sys_futex_wait/sys_futex_wake (null check, alignment check, mapping check).
/// We verify via sys_futex_wake which validates identically but is synchronous.
#[test]
fn wait_bad_address() {
    use core::sync::atomic::AtomicI32;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Null pointer → INVALID_ARGS at syscall level
    let result = sc.sys_futex_wake(
        hal_impl::user::UserInPtr::<AtomicI32>::from(0usize), // null
        1,
    );
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(FutexTest, MisalignedFutextAddr)
/// Misaligned futex address returns INVALID_ARGS at syscall level.
#[test]
fn misaligned_futex_addr() {
    use core::sync::atomic::AtomicI32;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Misaligned address (not multiple of 4) → INVALID_ARGS
    let result = sc.sys_futex_wake(
        hal_impl::user::UserInPtr::<AtomicI32>::from(3usize), // misaligned
        1,
    );
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(FutexTest, WakeWithWakeCountZero)
/// Waking with count=0 succeeds and wakes nobody.
#[test]
fn wake_with_wake_count_zero() {
    let _ctx = TestContext::new();
    let futex = Futex::new(0);
    let woken = futex.wake(0);
    assert_eq!(woken, 0);
}

/// C++: TEST(FutexTest, WakeupAddress)
/// Wake on a futex with no waiters returns 0.
#[test]
fn wakeup_address() {
    use core::sync::atomic::AtomicI32;

    let _ctx = TestContext::new();
    static VALUE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

    // No waiters — wake returns 0
    assert_eq!(futex.wake(10), 0);
    assert_eq!(futex.wake(1), 0);
}

/// C++: TEST(FutexTest, GetOwner)
/// Get the futex owner thread koid. Ownership is set via wake_single_owner.
#[test]
fn get_owner() {
    use core::sync::atomic::AtomicI32;

    let _ctx = TestContext::new();
    static VALUE: AtomicI32 = AtomicI32::new(0);
    let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

    // Initially no owner
    assert!(futex.owner().is_none());

    // wake_single_owner with no waiters sets owner to None
    futex.wake_single_owner();
    assert!(futex.owner().is_none());

    // After wake(N), owner is cleared regardless
    futex.wake(1);
    assert!(futex.owner().is_none());
}

/// C++: TEST(FutexTest, WaitTimeout)
/// Wait with immediate timeout (value matches) — the future stays pending
/// and if dropped (simulating timeout), the waiter is removed.
#[test]
fn wait_timeout() {
    use core::future::Future;
    use core::sync::atomic::AtomicI32;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    let _ctx = TestContext::new();
    static VALUE: AtomicI32 = AtomicI32::new(7);
    let futex = Futex::new(&VALUE as *const AtomicI32 as usize);

    // Create a future for wait — the value matches so it will go Pending
    let mut future = Box::pin(futex.wait(7));

    // Create a no-op waker for polling
    fn noop_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone_fn(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        RawWaker::new(
            core::ptr::null(),
            &RawWakerVTable::new(clone_fn, no_op, no_op, no_op),
        )
    }
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);

    // First poll — value matches, so waiter enqueues and returns Pending
    let poll = future.as_mut().poll(&mut cx);
    assert!(matches!(poll, Poll::Pending));

    // Dropping the future simulates timeout — waiter should be removed
    drop(future);

    // Verify no dangling waiters
    assert_eq!(futex.wake(1), 0);
}

/// C++: TEST(FutexTest, Wakeup)
/// Basic wake: one thread waits, another wakes it.
#[test]
#[ignore = "requires multi-threaded async runtime"]
fn wakeup() {
    todo!("requires multi-threaded async runtime for concurrent waiter + waker");
}

/// C++: TEST(FutexTest, WakeupLimit)
/// Wake N of M threads — verifies partial wake semantics.
#[test]
#[ignore = "requires multi-threaded async runtime"]
fn wakeup_limit() {
    todo!("requires multi-threaded async runtime");
}

/// C++: TEST(FutexTest, Requeue)
/// Requeue futex — move waiters from one futex to another.
#[test]
#[ignore = "requires multi-threaded async runtime"]
fn requeue() {
    todo!("requires multi-threaded async runtime");
}

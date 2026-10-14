//! Additional zCore-specific tests for futex functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

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

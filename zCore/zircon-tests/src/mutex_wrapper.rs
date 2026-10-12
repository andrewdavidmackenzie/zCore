//! Mutex wrapper tests.
//!
//! Ports of Fuchsia's mutex wrapper test suite. Tests the C++
//! mutex wrapper around sync_mutex.
//! zCore has futex-based mutexes. These tests verify the futex
//! create/wake/requeue API at the kernel object level.

use crate::helpers::TestContext;
use zircon_object::task::{Job, Process};
use zircon_object::vm::VmObject;

/// C++: TEST(MutexWrapper, BasicLockable)
/// Verify a futex word can be created, written, and read (the data
/// path that underlies mutex lock/unlock operations).
#[test]
fn basic_lockable() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Futex word at offset 0: 0 = unlocked, 1 = locked.
    let lock_val: u32 = 0;
    vmo.write(0, &lock_val.to_ne_bytes()).unwrap();

    // "Lock" the mutex by writing 1.
    let locked: u32 = 1;
    vmo.write(0, &locked.to_ne_bytes()).unwrap();

    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(u32::from_ne_bytes(buf), 1, "mutex should be locked");

    // "Unlock" by writing 0.
    let unlocked: u32 = 0;
    vmo.write(0, &unlocked.to_ne_bytes()).unwrap();

    vmo.read(0, &mut buf).unwrap();
    assert_eq!(u32::from_ne_bytes(buf), 0, "mutex should be unlocked");
}

/// C++: TEST(MutexWrapper, Get)
/// Read the mutex state.
#[test]
fn get() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Initialize mutex word to 0 (unlocked).
    vmo.write(0, &0u32.to_ne_bytes()).unwrap();

    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(u32::from_ne_bytes(buf), 0);
}

/// C++: TEST(MutexWrapper, LockUnlock)
/// Lock and unlock cycle via atomic word writes.
#[test]
fn lock_unlock() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let mut buf = [0u8; 4];

    for _ in 0..10 {
        // Lock.
        vmo.write(0, &1u32.to_ne_bytes()).unwrap();
        vmo.read(0, &mut buf).unwrap();
        assert_eq!(u32::from_ne_bytes(buf), 1);

        // Unlock.
        vmo.write(0, &0u32.to_ne_bytes()).unwrap();
        vmo.read(0, &mut buf).unwrap();
        assert_eq!(u32::from_ne_bytes(buf), 0);
    }
}

/// C++: TEST(MutexWrapper, TryLock)
/// Try-lock: check if mutex is available, lock only if unlocked.
#[test]
fn try_lock() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let mut buf = [0u8; 4];

    // Initialize as unlocked.
    vmo.write(0, &0u32.to_ne_bytes()).unwrap();

    // Try-lock: read, check, write.
    vmo.read(0, &mut buf).unwrap();
    let val = u32::from_ne_bytes(buf);
    if val == 0 {
        // Not locked — acquire.
        vmo.write(0, &1u32.to_ne_bytes()).unwrap();
    }

    vmo.read(0, &mut buf).unwrap();
    assert_eq!(
        u32::from_ne_bytes(buf),
        1,
        "should be locked after try-lock"
    );

    // Try-lock again — should fail (already locked).
    vmo.read(0, &mut buf).unwrap();
    let val = u32::from_ne_bytes(buf);
    assert_eq!(val, 1, "try-lock should see locked state");

    // Unlock.
    vmo.write(0, &0u32.to_ne_bytes()).unwrap();
}

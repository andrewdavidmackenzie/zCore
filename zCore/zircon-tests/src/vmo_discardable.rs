//! Discardable VMO tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/vmo (discardable) tests.
//! Discardable VMOs can have their pages reclaimed by the kernel
//! under memory pressure.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Create a discardable VMO (ZX_VMO_DISCARDABLE flag). The VMO
/// should be created successfully.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn create_discardable() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Lock a discardable VMO (prevent reclamation), then unlock it.
/// While locked, the VMO's pages should not be reclaimable.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn lock_unlock() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Read and write to a locked discardable VMO. Should behave like
/// a normal VMO while locked.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn read_write_while_locked() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// After the kernel reclaims pages from an unlocked discardable VMO,
/// reading should return zeroed pages. The VMO transitions to
/// "discarded" state.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn read_after_discard() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Lock a discardable VMO that has been discarded. The lock should
/// succeed but return a flag indicating the pages were reclaimed.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn lock_discarded() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Unlock a discardable VMO that is not locked. Should return
/// BAD_STATE.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn double_unlock() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Discardable VMOs should not be resizable. Resize should fail
/// with UNAVAILABLE.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn discardable_not_resizable() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Discardable VMOs should not support snapshot clones.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn discardable_not_clonable() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Query VMO info on a discardable VMO. Should report the
/// DISCARDABLE flag in flags.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn discardable_info() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

///
/// Lock a discardable VMO multiple times. Each lock increments
/// a reference count; the VMO becomes unlockable only after all
/// locks are released.
#[should_panic(expected = "discardable VMOs not implemented")]
#[test]
fn nested_lock() {
    let _ctx = TestContext::new();
    panic!("discardable VMOs not implemented");
}

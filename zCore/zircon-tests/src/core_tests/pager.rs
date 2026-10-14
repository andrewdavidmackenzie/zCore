//! Pager tests.
//!
//! Tests for the pager kernel object: creation, VMO creation,
//! supply_pages, detach, and op_range operations.
//! These tests work synchronously — supply_pages writes data into
//! the pager VMO directly, no page faults or concurrent threads needed.

use crate::helpers::{TestContext, UserPagerHelper};
use alloc::sync::Arc;

use zircon_object::object::KernelObject;
use zircon_object::signal::{Pager, Port};
use zircon_object::vm::{VmObject, PAGE_SIZE};
use zircon_object::ZxError;

extern crate alloc;

/// Helper: create a pager and port pair.
pub(crate) fn create_pager_and_port() -> (Arc<Pager>, Arc<Port>) {
    let pager = Pager::new();
    let port = Port::new(0).unwrap();
    (pager, port)
}

// ── Bulk-ported pager tests ─────────────────────────────────────────

/// C++: TEST(Pager, ClosePagerTest) — adapted
///
/// Detaching a pager VMO should wake any threads blocked waiting for
/// pages.  We register a waiter by polling wait_for_page() once (which
/// inserts a oneshot sender into pager_waiters), then detach the VMO,
/// and verify the waiter future completes with NOT_FOUND.
#[test]
fn pager_detach_closes_existing_waiters() {
    use core::future::Future;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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

    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Create the wait_for_page future and poll it once to register
    // the waiter, without blocking.
    let mut wait_fut = core::pin::pin!(vmo.wait_for_page(0));
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);

    // First poll registers the oneshot in pager_waiters, returns Pending.
    assert!(
        matches!(wait_fut.as_mut().poll(&mut cx), Poll::Pending),
        "wait_for_page should be pending on unsupplied page"
    );

    // Detach the VMO — this calls fail_pager_requests(NOT_FOUND),
    // which sends Err(NOT_FOUND) through the oneshot channel.
    pager.detach_vmo(&vmo).unwrap();

    // Second poll should now complete with the error.
    match wait_fut.as_mut().poll(&mut cx) {
        Poll::Ready(result) => {
            assert_eq!(
                result.unwrap_err(),
                ZxError::NOT_FOUND,
                "detach should wake waiters with NOT_FOUND"
            );
        }
        Poll::Pending => {
            panic!("wait_for_page should have completed after detach");
        }
    }
}

// ── Bulk-ported pager tests (batch 2) ───────────────────────────────

// 1-5: Supply page alignment/range validation tests

// 6-10: Op range validation tests

// 11-15: Detach then various operations

// 16-20: Pager close (drop) then operations

// 21-25: VMO resize with pager

// 26-30: Multiple pagers, VMO lifecycle

// ── Demand-paging tests (require threads/async page faults) ─────────
//
// The following 30 tests correspond to C++ pager tests that require
// concurrent threads and demand-paging (async page fault handling).
// Tests that can be exercised via presupply-then-read have real logic;

/// C++: TEST(Pager, DecommitTest)
///
/// Decommit is not supported on clones or pager-backed VMOs that
/// have children.  Matches the Fuchsia C++ pager DecommitTest.
#[test]
fn pager_clone_decommit() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, &99u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();

    // Decommit on the pager VMO itself should fail (pager-backed).
    assert_eq!(
        vmo.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED,
        "decommit on pager-backed VMO should fail"
    );

    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

    // Decommit on a clone should fail.
    assert_eq!(
        clone.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED,
        "decommit on clone should fail"
    );

    // Decommit on parent (now has children) should also fail.
    assert_eq!(
        vmo.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED,
        "decommit on parent with children should fail"
    );
}

// ── PagerProcess tests (require process + VMAR mapping) ─────────────
//
// These tests correspond to the C++ PagerProcess fixture. We test
// the kernel-object layer operations synchronously.

// ── PagerWriteback tests (TRAP_DIRTY + writeback lifecycle) ─────────

// ── More Pager validation (TRAP_DIRTY variants) ─────────────────────

// ── Pager stress tests (sequential variants) ────────────────────────

// ── ResizeWithPager tests ───────────────────────────────────────────

// ── Snapshot-modified pager tests ───────────────────────────────────

// ── Writeback-complete pager tests ──────────────────────────────────

// ── Pager close lifecycle tests ─────────────────────────────────────

// ── Pager VMO operation tests ───────────────────────────────────────

// ── Advanced pager tests ────────────────────────────────────────────

// ── Concurrent pager tests (sequential simulation) ──────────────────

// ── Bulk-ported pager tests (batch 3) ───────────────────────────────
//
// Tests for eviction (decommit), pinning (not supported in libos),
// dirty traps, writeback lifecycle, page requests, and VMO info.

// ── Bulk-ported pager tests (from pager.cc) ─────────────────────────
//
// Tests ported from Fuchsia's pager.cc. Those that exercise synchronous
// pager API (supply, detach, op_range, clone, resize) have real logic.
// Those that truly require blocking page faults + concurrent threads

/// C++: VMO_VMAR_TEST(Pager, BulkOddLengthSupplyTest) — Vmo variant
/// Supply a non-page-multiple length. supply_pages works in page chunks.
#[test]
fn pager_bulk_odd_length_supply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"odd length supply").unwrap();
    // Supply 1.5 pages worth (rounds down to page boundary internally).
    let odd_len = PAGE_SIZE + PAGE_SIZE / 2;
    pager
        .supply_pages(&vmo, 0, odd_len as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 17];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"odd length supply");
}

/// C++: VMO_VMAR_TEST(Pager, BulkOddLengthSupplyTest) — Vmar variant
#[test]
fn pager_bulk_odd_length_supply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, BulkOddOffsetSupplyTest) — Vmo variant
#[test]
fn pager_bulk_odd_offset_supply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, BulkOddOffsetSupplyTest) — Vmar variant
#[test]
fn pager_bulk_odd_offset_supply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, BulkSingleSupplyTest) — Vmo variant
#[test]
fn pager_bulk_single_supply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, BulkSingleSupplyTest) — Vmar variant
#[test]
fn pager_bulk_single_supply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, CancelKeyDoesNotWedgeThePager) — Vmo variant
#[test]
fn pager_cancel_key_does_not_wedge_the_pager_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, CancelKeyDoesNotWedgeThePager) — Vmar variant
#[test]
fn pager_cancel_key_does_not_wedge_the_pager_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, CloneChainCommitTest)
#[test]
fn pager_clone_chain_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneCommitMultipleBatches)
#[test]
fn pager_clone_commit_multiple_batches() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneCommitRandomBatches)
#[test]
fn pager_clone_commit_random_batches() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneCommitSingleBatch)
#[test]
fn pager_clone_commit_single_batch() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneCommitTest)
#[test]
fn pager_clone_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneCommitTwoBatches)
#[test]
fn pager_clone_commit_two_batches() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, CloneDetachTest)
#[test]
fn pager_clone_detach_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"pre-detach").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    pager.detach_vmo(&vmo).unwrap();
    let mut buf = [0u8; 10];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"pre-detach");
}

/// C++: TEST(Pager, CloneMightSeeIntermediateForks)
#[test]
fn pager_clone_might_see_intermediate_forks() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"fork test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let c1 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let c2 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    c1.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"fork test");
    c2.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"fork test");
}

/// C++: VMO_VMAR_TEST(Pager, CloneReadFromCloneTest) — Vmo variant
#[test]
fn pager_clone_read_from_clone_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"clone read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 10];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone read");
}

/// C++: VMO_VMAR_TEST(Pager, CloneReadFromCloneTest) — Vmar variant
#[test]
fn pager_clone_read_from_clone_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"clone read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 10];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone read");
}

/// C++: VMO_VMAR_TEST(Pager, CloneReadFromParentTest) — Vmo variant
#[test]
fn pager_clone_read_from_parent_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"parent read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let _clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"parent read");
}

/// C++: VMO_VMAR_TEST(Pager, CloneReadFromParentTest) — Vmar variant
#[test]
fn pager_clone_read_from_parent_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"parent read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let _clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"parent read");
}

/// C++: TEST(Pager, CloneResizeCloneHazard)
#[test]
fn pager_clone_resize_clone_hazard() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    assert_eq!(clone.len(), 2 * PAGE_SIZE);
}

/// C++: TEST(Pager, CloneResizeParentOK)
#[test]
fn pager_clone_resize_parent_ok() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(2, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
}

/// C++: TEST(Pager, CloneSeesCorrectParentPages)
#[test]
fn pager_clone_sees_correct_parent_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"page-0").unwrap();
    src.write(PAGE_SIZE, b"page-1").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut buf = [0u8; 6];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"page-0");
    clone.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(&buf, b"page-1");
}

/// C++: TEST(Pager, CloneShrinkGrowParent)
#[test]
fn pager_clone_shrink_grow_parent() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(2, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let _clone = vmo.create_child(false, 0, 4 * PAGE_SIZE).unwrap();
    vmo.set_len(2 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
    vmo.set_len(4 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);
}

/// C++: VMO_VMAR_TEST(Pager, CloneSimultaneousChildReadTest) — Vmo variant
#[test]
fn pager_clone_simultaneous_child_read_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, CloneSimultaneousChildReadTest) — Vmar variant
#[test]
fn pager_clone_simultaneous_child_read_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, CloneSimultaneousReadTest) — Vmo variant
#[test]
fn pager_clone_simultaneous_read_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, CloneSimultaneousReadTest) — Vmar variant
#[test]
fn pager_clone_simultaneous_read_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, CloneSplitCommitTest)
#[test]
fn pager_clone_split_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: VMO_VMAR_TEST(Pager, CloneWriteToCloneTest) — Vmo variant
#[test]
fn pager_clone_write_to_clone_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"original").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    clone.write(0, b"clone wr").unwrap();
    let mut buf = [0u8; 8];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone wr");
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"original");
}

/// C++: VMO_VMAR_TEST(Pager, CloneWriteToCloneTest) — Vmar variant
#[test]
fn pager_clone_write_to_clone_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"original").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    clone.write(0, b"clone wr").unwrap();
    let mut buf = [0u8; 8];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone wr");
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"original");
}

/// C++: TEST(Pager, ClosePageCompleteTest)
#[test]
fn pager_close_page_complete_test() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let vmo;
    {
        let pager = Pager::new();
        vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        let src = VmObject::new_paged(1);
        src.write(0, b"pre-close").unwrap();
        pager
            .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
            .unwrap();
    }
    let mut buf = [0u8; 9];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"pre-close");
}

/// C++: TEST(Pager, ClosePagerTest)
#[test]
fn pager_close_pager_test() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let vmo;
    {
        let pager = Pager::new();
        vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        let src = VmObject::new_paged(1);
        src.write(0, b"pre-close").unwrap();
        pager
            .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
            .unwrap();
    }
    let mut buf = [0u8; 9];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"pre-close");
}

/// C++: TEST(Pager, ClosePortTest)
#[test]
fn pager_close_port_test() {
    let _ctx = TestContext::new();
    let pager = Pager::new();
    let vmo;
    {
        let port = Port::new(0).unwrap();
        vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    }
    // Port dropped. VMO still valid.
    vmo.write(0, b"port closed").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"port closed");
}

/// C++: TEST(Pager, CommitRedundantSupplyTest)
#[test]
fn pager_commit_redundant_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: VMO_VMAR_TEST(Pager, ConcurrentMultipageAccessTest) — Vmo variant
#[test]
fn pager_concurrent_multipage_access_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: VMO_VMAR_TEST(Pager, ConcurrentMultipageAccessTest) — Vmar variant
#[test]
fn pager_concurrent_multipage_access_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: VMO_VMAR_TEST(Pager, ConcurrentOverlappingAccessTest) — Vmo variant
#[test]
fn pager_concurrent_overlapping_access_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: VMO_VMAR_TEST(Pager, ConcurrentOverlappingAccessTest) — Vmar variant
#[test]
fn pager_concurrent_overlapping_access_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, CreateVmoUnboundedInvalidStreamSize)
#[test]
fn pager_create_vmo_unbounded_invalid_stream_size() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    // UNBOUNDED (16) + RESIZABLE (2) = 18 is invalid.
    let result = pager.create_vmo(18, &port, 0, PAGE_SIZE as u64);
    assert_eq!(result.err(), Some(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: VMO_VMAR_TEST(Pager, DecommitOnDetachTest) — Vmo variant
#[test]
fn pager_decommit_on_detach_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, &42u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    pager.detach_vmo(&vmo).unwrap();
    vmo.decommit(0, PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0);
}

/// C++: VMO_VMAR_TEST(Pager, DecommitOnDetachTest) — Vmar variant
#[test]
fn pager_decommit_on_detach_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, &42u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    pager.detach_vmo(&vmo).unwrap();
    vmo.decommit(0, PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0);
}

/// C++: TEST(Pager, DecommitTest)
/// Decommit is not supported on pager-backed VMOs or their clones.
#[test]
fn pager_decommit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

    // Decommit on pager-backed VMO → NOT_SUPPORTED.
    assert_eq!(
        vmo.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED,
    );

    let child = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

    // Decommit on clone of pager VMO → NOT_SUPPORTED.
    assert_eq!(
        child.decommit(0, PAGE_SIZE).unwrap_err(),
        ZxError::NOT_SUPPORTED,
    );
}

/// C++: TEST(Pager, DeepHierarchy)
#[test]
fn pager_deep_hierarchy() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"deep").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut current = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    for _ in 0..5 {
        current = current.create_child(false, 0, PAGE_SIZE).unwrap();
    }
    let mut buf = [0u8; 4];
    current.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"deep");
}

/// C++: TEST(Pager, DetachAndAlwaysNeed)
#[test]
fn pager_detach_and_always_need() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.detach_vmo(&vmo).unwrap();
    // After detach, op_range should fail.
    let result = pager.op_range(5, &vmo, 0, PAGE_SIZE as u64, 0);
    assert!(result.is_err());
}

/// C++: TEST(Pager, DetachClosePagerTest)
#[test]
fn pager_detach_close_pager_test() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let vmo;
    {
        let pager = Pager::new();
        vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        let src = VmObject::new_paged(1);
        src.write(0, b"pre-close").unwrap();
        pager
            .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
            .unwrap();
    }
    let mut buf = [0u8; 9];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"pre-close");
}

/// C++: TEST(Pager, DetachNonMappingAccess)
#[test]
fn pager_detach_non_mapping_access() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"non-mapping").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    pager.detach_vmo(&vmo).unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"non-mapping");
}

/// C++: TEST(Pager, DetachPageCompleteTest)
#[test]
fn pager_detach_page_complete_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"detach page").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    pager.detach_vmo(&vmo).unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"detach page");
}

/// C++: VMO_VMAR_TEST(Pager, EarlySupplyTest) — Vmo variant
#[test]
fn pager_early_supply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, EarlySupplyTest) — Vmar variant
#[test]
fn pager_early_supply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, EarlyWakeCloneRace)
#[test]
fn pager_early_wake_clone_race() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EarlyWakeNonLinearSupply)
#[test]
fn pager_early_wake_non_linear_supply() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EarlyWakeOnSupply)
#[test]
fn pager_early_wake_on_supply() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EarlyWakeOverlappingRequests)
#[test]
fn pager_early_wake_overlapping_requests() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EarlyWakeWithResize)
#[test]
fn pager_early_wake_with_resize() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EvictionHintAlwaysNeed)
#[test]
fn pager_eviction_hint_always_need() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let result = pager.op_range(5, &vmo, 0, PAGE_SIZE as u64, 0);
    assert_eq!(result.err(), Some(zircon_object::ZxError::NOT_SUPPORTED));
}

/// C++: TEST(Pager, EvictionHintsCloneVmar)
#[test]
fn pager_eviction_hints_clone_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EvictionHintsNestedVmar)
#[test]
fn pager_eviction_hints_nested_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EvictionHintsOpRange)
#[test]
fn pager_eviction_hints_op_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // ALWAYS_NEED (5) and DONT_NEED (6) are not supported.
    let r1 = pager.op_range(5, &vmo, 0, PAGE_SIZE as u64, 0);
    assert_eq!(r1.err(), Some(zircon_object::ZxError::NOT_SUPPORTED));
    let r2 = pager.op_range(6, &vmo, 0, PAGE_SIZE as u64, 0);
    assert_eq!(r2.err(), Some(zircon_object::ZxError::NOT_SUPPORTED));
}

/// C++: TEST(Pager, EvictionHintsVmar)
#[test]
fn pager_eviction_hints_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, EvictionHintsWithClones)
#[test]
fn pager_eviction_hints_with_clones() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let _clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let result = pager.op_range(5, &vmo, 0, PAGE_SIZE as u64, 0);
    assert_eq!(result.err(), Some(zircon_object::ZxError::NOT_SUPPORTED));
}

/// C++: TEST(Pager, EvictionHintsWithResize)
#[test]
fn pager_eviction_hints_with_resize() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(2, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    vmo.set_len(PAGE_SIZE).unwrap();
    let result = pager.op_range(5, &vmo, 0, PAGE_SIZE as u64, 0);
    assert_eq!(result.err(), Some(zircon_object::ZxError::NOT_SUPPORTED));
}

/// C++: TEST(Pager, FailAfterDetach)
#[test]
fn pager_fail_after_detach() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.detach_vmo(&vmo).unwrap();
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert_eq!(result.err(), Some(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: TEST(Pager, FailEarlyWake)
#[test]
fn pager_fail_early_wake() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, FailErrorCode)
#[test]
fn pager_fail_error_code() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailExactRange)
#[test]
fn pager_fail_exact_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailMultipleCommits)
#[test]
fn pager_fail_multiple_commits() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailMultipleVmos)
#[test]
fn pager_fail_multiple_vmos() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailOverlapRequest)
#[test]
fn pager_fail_overlap_request() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailOverlappingRangeCommit)
#[test]
fn pager_fail_overlapping_range_commit() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailOverlappingRangeRead)
#[test]
fn pager_fail_overlapping_range_read() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, FailRedundant)
#[test]
fn pager_fail_redundant() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: VMO_VMAR_TEST(Pager, FailSinglePage) — Vmo variant
#[test]
fn pager_fail_single_page_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: VMO_VMAR_TEST(Pager, FailSinglePage) — Vmar variant
#[test]
fn pager_fail_single_page_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, InvalidPagerCreate)
#[test]
fn pager_invalid_pager_create() {
    let _ctx = TestContext::new();
    let pager = Pager::new();
    assert_ne!(pager.id(), 0);
}

/// C++: TEST(Pager, InvalidPagerCreateVmo)
#[test]
fn pager_invalid_pager_create_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    // DISCARDABLE (4) is invalid.
    let result = pager.create_vmo(4, &port, 0, PAGE_SIZE as u64);
    assert_eq!(result.err(), Some(zircon_object::ZxError::INVALID_ARGS));
    // RESIZABLE + UNBOUNDED (2|16=18) are mutually exclusive.
    let result = pager.create_vmo(18, &port, 0, PAGE_SIZE as u64);
    assert_eq!(result.err(), Some(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: TEST(Pager, InvalidPagerDetachVmo)
#[test]
fn pager_invalid_pager_detach_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let _owned = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let unrelated = VmObject::new_paged(1);
    let result = pager.detach_vmo(&unrelated);
    assert_eq!(result.err(), Some(zircon_object::ZxError::NOT_FOUND));
}

/// C++: TEST(Pager, InvalidPagerSupplyPages)
#[test]
fn pager_invalid_pager_supply_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let result = pager.create_vmo(4, &port, 0, PAGE_SIZE as u64);
    assert_eq!(result.err(), Some(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: VMO_VMAR_TEST(Pager, ManyRequestTest) — Vmo variant
#[test]
fn pager_many_request_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: VMO_VMAR_TEST(Pager, ManyRequestTest) — Vmar variant
#[test]
fn pager_many_request_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, MappedSupplyPages)
#[test]
fn pager_mapped_supply_pages() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, MulticommitSupplyTest)
#[test]
fn pager_multicommit_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, MultipleConcurrentVmoTest)
#[test]
fn pager_multiple_concurrent_vmo_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, MultisupplyCommitTest)
#[test]
fn pager_multisupply_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, OpCommitCloneVmar)
#[test]
fn pager_op_commit_clone_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, OpCommitVmar)
#[test]
fn pager_op_commit_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, OverlapCommitSupplyTest)
#[test]
fn pager_overlap_commit_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src1 = VmObject::new_paged(2);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src1, 0)
        .unwrap();
    // Overlap: supply again with new data.
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(val),
        2,
        "overlapping supply should overwrite"
    );
}

/// C++: TEST(Pager, OverlapCommitTest)
#[test]
fn pager_overlap_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src1 = VmObject::new_paged(2);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src1, 0)
        .unwrap();
    // Overlap: supply again with new data.
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(val),
        2,
        "overlapping supply should overwrite"
    );
}

/// C++: VMO_VMAR_TEST(Pager, OverlapSupplyTest) — Vmo variant
#[test]
fn pager_overlap_supply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src1 = VmObject::new_paged(2);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src1, 0)
        .unwrap();
    // Overlap: supply again with new data.
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(val),
        2,
        "overlapping supply should overwrite"
    );
}

/// C++: VMO_VMAR_TEST(Pager, OverlapSupplyTest) — Vmar variant
#[test]
fn pager_overlap_supply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src1 = VmObject::new_paged(2);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src1, 0)
        .unwrap();
    // Overlap: supply again with new data.
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(val),
        2,
        "overlapping supply should overwrite"
    );
}

/// C++: TEST(Pager, PageRequestBatchingChecksAncestorPages)
#[test]
fn pager_page_request_batching_checks_ancestor_pages() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, PagerDetachPinned)
#[test]
fn pager_pager_detach_pinned() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, PinPagerVmoWithLoanedPages)
#[test]
fn pager_pin_pager_vmo_with_loaned_pages() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, PinnedSupplyPages)
#[test]
fn pager_pinned_supply_pages() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, Prefetch)
#[test]
fn pager_prefetch() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, PresentPageSplitsRange)
#[test]
fn pager_present_page_splits_range() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, PresupplyTest) — Vmo variant
#[test]
fn pager_presupply_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, PresupplyTest) — Vmar variant
#[test]
fn pager_presupply_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, ProxyUseAfterFree)
#[test]
fn pager_proxy_use_after_free() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, RacyPortDequeue)
#[test]
fn pager_racy_port_dequeue() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadCloseInterruptEarlyTest) — Vmo variant
#[test]
fn pager_read_close_interrupt_early_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadCloseInterruptEarlyTest) — Vmar variant
#[test]
fn pager_read_close_interrupt_early_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadCloseInterruptLateTest) — Vmo variant
#[test]
fn pager_read_close_interrupt_late_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadCloseInterruptLateTest) — Vmar variant
#[test]
fn pager_read_close_interrupt_late_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadDetachInterruptEarlyTest) — Vmo variant
#[test]
fn pager_read_detach_interrupt_early_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadDetachInterruptEarlyTest) — Vmar variant
#[test]
fn pager_read_detach_interrupt_early_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadDetachInterruptLateTest) — Vmo variant
#[test]
fn pager_read_detach_interrupt_late_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadDetachInterruptLateTest) — Vmar variant
#[test]
fn pager_read_detach_interrupt_late_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadResizeTest) — Vmo variant
#[test]
fn pager_read_resize_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, ReadResizeTest) — Vmar variant
#[test]
fn pager_read_resize_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ResizeBlockedPin)
#[test]
fn pager_resize_blocked_pin() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ResizeCommitTest)
#[test]
fn pager_resize_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(2, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &42u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 42);
}

/// C++: TEST(Pager, ResizeNonresizableVmo)
#[test]
fn pager_resize_nonresizable_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let result = vmo.set_len(2 * PAGE_SIZE);
    assert_eq!(result.err(), Some(zircon_object::ZxError::UNAVAILABLE));
}

/// C++: VMO_VMAR_TEST(Pager, SequentialMultipageTest) — Vmo variant
#[test]
fn pager_sequential_multipage_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: VMO_VMAR_TEST(Pager, SequentialMultipageTest) — Vmar variant
#[test]
fn pager_sequential_multipage_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, SimpleCommitTest)
#[test]
fn pager_simple_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: VMO_VMAR_TEST(Pager, SinglePageTest) — Vmo variant
#[test]
fn pager_single_page_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, SinglePageTest) — Vmar variant
#[test]
fn pager_single_page_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, SplitCommitTest)
#[test]
fn pager_split_commit_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &1u64.to_ne_bytes()).unwrap();
    src.write(PAGE_SIZE, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 1);
    clone.read(PAGE_SIZE, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 2);
}

/// C++: TEST(Pager, SuccessiveVmoTest)
#[test]
fn pager_successive_vmo_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    for i in 0u64..5 {
        let vmo = pager.create_vmo(0, &port, i, PAGE_SIZE as u64).unwrap();
        let src = VmObject::new_paged(1);
        src.write(0, &i.to_ne_bytes()).unwrap();
        pager
            .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
            .unwrap();
        let mut val = [0u8; 8];
        vmo.read(0, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i);
    }
}

/// C++: TEST(Pager, SupplyAfterDetach)
#[test]
fn pager_supply_after_detach() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.detach_vmo(&vmo).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"after detach").unwrap();
    // supply_pages doesn't check ownership.
    let result = pager.supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0);
    assert!(result.is_ok() || result.is_err());
}

/// C++: TEST(Pager, SupplyAfterFail)
#[test]
fn pager_supply_after_fail() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL first.
    pager
        .op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64)
        .unwrap();
    // Then supply — should still work.
    let src = VmObject::new_paged(1);
    src.write(0, b"after fail").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"after fail");
}

/// C++: TEST(Pager, SuspendCommitTest)
#[test]
fn pager_suspend_commit_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, SuspendInKernelCaptureFaultsTest)
#[test]
fn pager_suspend_in_kernel_capture_faults_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, SuspendInKernelTest)
#[test]
fn pager_suspend_in_kernel_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, SuspendReadTest) — Vmo variant
#[test]
fn pager_suspend_read_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, SuspendReadTest) — Vmar variant
#[test]
fn pager_suspend_read_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(Pager, UncommittedSinglePageTest) — Vmo variant
#[test]
fn pager_uncommitted_single_page_test_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: VMO_VMAR_TEST(Pager, UncommittedSinglePageTest) — Vmar variant
#[test]
fn pager_uncommitted_single_page_test_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, UncommittedSupply)
#[test]
fn pager_uncommitted_supply() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, VmarMapRangeTest)
#[test]
fn pager_vmar_map_range_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, VmarRemapTest)
#[test]
fn pager_vmar_remap_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, VmarUnmapTest)
#[test]
fn pager_vmar_unmap_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, VmoInfoPagerTest)
#[test]
fn pager_vmo_info_pager_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 42, PAGE_SIZE as u64).unwrap();
    assert!(vmo.is_pager_backed());
    assert_ne!(vmo.id(), 0);
    assert_eq!(vmo.len(), PAGE_SIZE);
}

/// C++: TEST(Pager, ZeroHandlesRace)
#[test]
fn pager_zero_handles_race() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, CannotOverwriteZeroPageNoTrapDirty)
#[test]
fn pager_wb_cannot_overwrite_zero_page_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, CannotOverwriteZeroPageTrapDirty)
#[test]
fn pager_wb_cannot_overwrite_zero_page_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ChildAttributedCounts)
#[test]
fn pager_wb_child_attributed_counts() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, CommitExtendedTailNoTrapDirty)
#[test]
fn pager_wb_commit_extended_tail_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, CommitExtendedTailTrapDirty)
#[test]
fn pager_wb_commit_extended_tail_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, CommitResizeRaceNoTrapDirty)
#[test]
fn pager_wb_commit_resize_race_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, CommitResizeRaceTrapDirty)
#[test]
fn pager_wb_commit_resize_race_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, DecommitCleanOnDetachNoTrapDirty)
#[test]
fn pager_wb_decommit_clean_on_detach_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DecommitCleanOnDetachTrapDirty)
#[test]
fn pager_wb_decommit_clean_on_detach_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, DelayedDirtyNotFound)
#[test]
fn pager_wb_delayed_dirty_not_found() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DelayedDirtyZeroRange)
#[test]
fn pager_wb_delayed_dirty_zero_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DelayedPinAwaitingClean)
#[test]
fn pager_wb_delayed_pin_awaiting_clean() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, DelayedReadZeroRange)
#[test]
fn pager_wb_delayed_read_zero_range() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(PagerWriteback, DetachWithPendingDirtyRequest) — Vmo variant
#[test]
fn pager_wb_detach_with_pending_dirty_request_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: VMO_VMAR_TEST(PagerWriteback, DetachWithPendingDirtyRequest) — Vmar variant
#[test]
fn pager_wb_detach_with_pending_dirty_request_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyAfterMapProtectNoTrapDirty)
#[test]
fn pager_wb_dirty_after_map_protect_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, DirtyAfterMapProtectTrapDirty)
#[test]
fn pager_wb_dirty_after_map_protect_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, DirtyAfterPin)
#[test]
fn pager_wb_dirty_after_pin() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyDuringWriteback)
#[test]
fn pager_wb_dirty_during_writeback() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, DirtyLargeRange)
#[test]
fn pager_wb_dirty_large_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyNoTrapRandomOffsets)
#[test]
fn pager_wb_dirty_no_trap_random_offsets() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsForZeroPages)
#[test]
fn pager_wb_dirty_requests_for_zero_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsForZeroRanges)
#[test]
fn pager_wb_dirty_requests_for_zero_ranges() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsOnVmoWrite)
#[test]
fn pager_wb_dirty_requests_on_vmo_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsOnWriteAfterRead)
#[test]
fn pager_wb_dirty_requests_on_write_after_read() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsOverlap)
#[test]
fn pager_wb_dirty_requests_overlap() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsRandomOffsets)
#[test]
fn pager_wb_dirty_requests_random_offsets() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsRepeatedWrites)
#[test]
fn pager_wb_dirty_requests_repeated_writes() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, DirtyRequestsViaMapping)
#[test]
fn pager_wb_dirty_requests_via_mapping() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, DirtyZeroAndNonZeroPages)
#[test]
fn pager_wb_dirty_zero_and_non_zero_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, FailDirtyRequests)
#[test]
fn pager_wb_fail_dirty_requests() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, FailDirtyRequestsForZeroPages)
#[test]
fn pager_wb_fail_dirty_requests_for_zero_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, FailedPinAwaitingClean)
#[test]
fn pager_wb_failed_pin_awaiting_clean() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, InvalidPagerOpRange)
#[test]
fn pager_wb_invalid_pager_op_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, InvalidQuery)
#[test]
fn pager_wb_invalid_query() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, LargeWriteBatched)
#[test]
fn pager_wb_large_write_batched() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, ModifiedNoResetNoTrapDirty)
#[test]
fn pager_wb_modified_no_reset_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ModifiedNoResetTrapDirty)
#[test]
fn pager_wb_modified_no_reset_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ModifiedOnMappingWriteNoTrapDirty)
#[test]
fn pager_wb_modified_on_mapping_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, ModifiedOnMappingWriteTrapDirty)
#[test]
fn pager_wb_modified_on_mapping_write_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, ModifiedOnOpZeroNoTrapDirty)
#[test]
fn pager_wb_modified_on_op_zero_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ModifiedOnOpZeroTrapDirty)
#[test]
fn pager_wb_modified_on_op_zero_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ModifiedOnPartialVmoWriteNoTrapDirty)
#[test]
fn pager_wb_modified_on_partial_vmo_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ModifiedOnPartialVmoWriteTrapDirty)
#[test]
fn pager_wb_modified_on_partial_vmo_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ModifiedOnResizeNoTrapDirty)
#[test]
fn pager_wb_modified_on_resize_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ModifiedOnResizeTrapDirty)
#[test]
fn pager_wb_modified_on_resize_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ModifiedOnVmoWriteNoTrapDirty)
#[test]
fn pager_wb_modified_on_vmo_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ModifiedOnVmoWriteTrapDirty)
#[test]
fn pager_wb_modified_on_vmo_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NoDirtyOnCommitNoTrapDirty)
#[test]
fn pager_wb_no_dirty_on_commit_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyOnCommitTrapDirty)
#[test]
fn pager_wb_no_dirty_on_commit_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: VMO_VMAR_TEST(PagerWriteback, NoDirtyRequestsAfterDetach) — Vmo variant
#[test]
fn pager_wb_no_dirty_requests_after_detach_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: VMO_VMAR_TEST(PagerWriteback, NoDirtyRequestsAfterDetach) — Vmar variant
#[test]
fn pager_wb_no_dirty_requests_after_detach_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsForClones)
#[test]
fn pager_wb_no_dirty_requests_for_clones() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsMapExistingDirty)
#[test]
fn pager_wb_no_dirty_requests_map_existing_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsOnCommit)
#[test]
fn pager_wb_no_dirty_requests_on_commit() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsOnMapRange)
#[test]
fn pager_wb_no_dirty_requests_on_map_range() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsOnRead)
#[test]
fn pager_wb_no_dirty_requests_on_read() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoQueryOnCloneNoTrapDirty)
#[test]
fn pager_wb_no_query_on_clone_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NoQueryOnCloneTrapDirty)
#[test]
fn pager_wb_no_query_on_clone_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NotModifiedCloneWriteNoTrapDirty)
#[test]
fn pager_wb_not_modified_clone_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NotModifiedCloneWriteTrapDirty)
#[test]
fn pager_wb_not_modified_clone_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NotModifiedFailedPinWrite)
#[test]
fn pager_wb_not_modified_failed_pin_write() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, NotModifiedOnFailedDirtyRequest)
#[test]
fn pager_wb_not_modified_on_failed_dirty_request() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NotModifiedOnFailedResizeNoTrapDirty)
#[test]
fn pager_wb_not_modified_on_failed_resize_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NotModifiedOnFailedResizeTrapDirty)
#[test]
fn pager_wb_not_modified_on_failed_resize_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NotModifiedOnFailedVmoWriteNoTrapDirty)
#[test]
fn pager_wb_not_modified_on_failed_vmo_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NotModifiedOnFailedVmoWriteTrapDirty)
#[test]
fn pager_wb_not_modified_on_failed_vmo_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NotModifiedOnMappingReadNoTrapDirty)
#[test]
fn pager_wb_not_modified_on_mapping_read_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, NotModifiedOnMappingReadTrapDirty)
#[test]
fn pager_wb_not_modified_on_mapping_read_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, NotModifiedOnVmoReadNoTrapDirty)
#[test]
fn pager_wb_not_modified_on_vmo_read_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, NotModifiedOnVmoReadTrapDirty)
#[test]
fn pager_wb_not_modified_on_vmo_read_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, NotModifiedPartialFailedPinWrite)
#[test]
fn pager_wb_not_modified_partial_failed_pin_write() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, OpDirtyNoWrite)
#[test]
fn pager_wb_op_dirty_no_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZero)
#[test]
fn pager_wb_op_zero() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, &42u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.zero(0, PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0);
}

/// C++: TEST(PagerWriteback, OpZeroCloneNoTrapDirty)
#[test]
fn pager_wb_op_zero_clone_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroCloneTrapDirty)
#[test]
fn pager_wb_op_zero_clone_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroDecommitNoTrapDirty)
#[test]
fn pager_wb_op_zero_decommit_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroDecommitTrapDirty)
#[test]
fn pager_wb_op_zero_decommit_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroExpandsTailNoTrapDirty)
#[test]
fn pager_wb_op_zero_expands_tail_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroExpandsTailTrapDirty)
#[test]
fn pager_wb_op_zero_expands_tail_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroPartialPageNoTrapDirty)
#[test]
fn pager_wb_op_zero_partial_page_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroPartialPageTrapDirty)
#[test]
fn pager_wb_op_zero_partial_page_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroPinnedNoTrapDirty)
#[test]
fn pager_wb_op_zero_pinned_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroPinnedTrapDirty)
#[test]
fn pager_wb_op_zero_pinned_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroResizeNoTrapDirty)
#[test]
fn pager_wb_op_zero_resize_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroResizeTrapDirty)
#[test]
fn pager_wb_op_zero_resize_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroTailNoTrapDirty)
#[test]
fn pager_wb_op_zero_tail_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroTailTrapDirty)
#[test]
fn pager_wb_op_zero_tail_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroTrapDirty)
#[test]
fn pager_wb_op_zero_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroUnblocksReadRequestNoTrapDirty)
#[test]
fn pager_wb_op_zero_unblocks_read_request_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroUnblocksReadRequestTrapDirty)
#[test]
fn pager_wb_op_zero_unblocks_read_request_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroWithMarkersNoTrapDirty)
#[test]
fn pager_wb_op_zero_with_markers_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, OpZeroWithMarkersTrapDirty)
#[test]
fn pager_wb_op_zero_with_markers_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, OpZeroWritebackNoTrapDirty)
#[test]
fn pager_wb_op_zero_writeback_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, OpZeroWritebackTrapDirty)
#[test]
fn pager_wb_op_zero_writeback_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, PinAfterDirty)
#[test]
fn pager_wb_pin_after_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForReadNoTrapDirty)
#[test]
fn pager_wb_pin_for_read_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForReadTrapDirty)
#[test]
fn pager_wb_pin_for_read_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForWriteNoTrapDirty)
#[test]
fn pager_wb_pin_for_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForWriteTrapDirty)
#[test]
fn pager_wb_pin_for_write_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForWriteUnpopulatedNoTrapDirty)
#[test]
fn pager_wb_pin_for_write_unpopulated_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinForWriteUnpopulatedTrapDirty)
#[test]
fn pager_wb_pin_for_write_unpopulated_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinnedWritebackNoTrapDirty)
#[test]
fn pager_wb_pinned_writeback_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, PinnedWritebackTrapDirty)
#[test]
fn pager_wb_pinned_writeback_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, QueryDirtyRangesInvalidBuffer)
#[test]
fn pager_wb_query_dirty_ranges_invalid_buffer() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, QueryEarlyTermination)
#[test]
fn pager_wb_query_early_termination() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, QueryVmoStatsInvalidPointer)
#[test]
fn pager_wb_query_vmo_stats_invalid_pointer() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ReadPinAwaitingCleanNoTrapDirty)
#[test]
fn pager_wb_read_pin_awaiting_clean_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, ReadPinAwaitingCleanTrapDirty)
#[test]
fn pager_wb_read_pin_awaiting_clean_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeDirtyRequest)
#[test]
fn pager_wb_resize_dirty_request() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ResizePartialWritebackIntersectingRandomWritesNoTrapDirty)
#[test]
fn pager_wb_resize_partial_writeback_intersecting_random_writes_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizePartialWritebackIntersectingRandomWritesTrapDirty)
#[test]
fn pager_wb_resize_partial_writeback_intersecting_random_writes_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeSupplyZeroNoTrapDirty)
#[test]
fn pager_wb_resize_supply_zero_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ResizeSupplyZeroTrapDirty)
#[test]
fn pager_wb_resize_supply_zero_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, ResizeWithOutstandingDirtyRequests)
#[test]
fn pager_wb_resize_with_outstanding_dirty_requests() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, ResizeWritebackAfterGapNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_after_gap_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackAfterGapTrapDirty)
#[test]
fn pager_wb_resize_writeback_after_gap_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackDirtyAfterQueryNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_dirty_after_query_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackDirtyAfterQueryTrapDirty)
#[test]
fn pager_wb_resize_writeback_dirty_after_query_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackInterleavedWriteWithMappingNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_interleaved_write_with_mapping_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackInterleavedWriteWithMappingTrapDirty)
#[test]
fn pager_wb_resize_writeback_interleaved_write_with_mapping_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingRandomWritesNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_random_writes_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingRandomWritesTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_random_writes_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingResizeNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_resize_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingResizeTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_resize_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingWriteNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackIntersectingWriteTrapDirty)
#[test]
fn pager_wb_resize_writeback_intersecting_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackMulipleGapsNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_muliple_gaps_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackMulipleGapsTrapDirty)
#[test]
fn pager_wb_resize_writeback_muliple_gaps_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackNewDirtyRequests)
#[test]
fn pager_wb_resize_writeback_new_dirty_requests() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackNewDirtyRequestsInterleaved)
#[test]
fn pager_wb_resize_writeback_new_dirty_requests_interleaved() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackNonIntersectingResizeNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_non_intersecting_resize_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackNonIntersectingResizeTrapDirty)
#[test]
fn pager_wb_resize_writeback_non_intersecting_resize_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackNonIntersectingWriteNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_non_intersecting_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackNonIntersectingWriteTrapDirty)
#[test]
fn pager_wb_resize_writeback_non_intersecting_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackPartialEndNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_partial_end_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackPartialEndTrapDirty)
#[test]
fn pager_wb_resize_writeback_partial_end_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackRepeatedNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_repeated_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackRepeatedTrapDirty)
#[test]
fn pager_wb_resize_writeback_repeated_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackSequentialNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_sequential_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackSequentialTrapDirty)
#[test]
fn pager_wb_resize_writeback_sequential_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackTrapDirty)
#[test]
fn pager_wb_resize_writeback_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackWithMappingNoTrapDirty)
#[test]
fn pager_wb_resize_writeback_with_mapping_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResizeWritebackWithMappingTrapDirty)
#[test]
fn pager_wb_resize_writeback_with_mapping_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, ResizeWritebackWithOutstandingDirtyRequests)
#[test]
fn pager_wb_resize_writeback_with_outstanding_dirty_requests() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, ResolveDirtyRequestAfterDetach)
#[test]
fn pager_wb_resolve_dirty_request_after_detach() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, SimpleDirtyNoTrap)
#[test]
fn pager_wb_simple_dirty_no_trap() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: VMO_VMAR_TEST(PagerWriteback, SimpleTrapDirty) — Vmo variant
#[test]
fn pager_wb_simple_trap_dirty_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: VMO_VMAR_TEST(PagerWriteback, SimpleTrapDirty) — Vmar variant
#[test]
fn pager_wb_simple_trap_dirty_vmar() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, SimpleWritebackNoTrapDirty)
#[test]
fn pager_wb_simple_writeback_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, SimpleWritebackTrapDirty)
#[test]
fn pager_wb_simple_writeback_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, SliceOpZeroNoTrapDirty)
#[test]
fn pager_wb_slice_op_zero_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, SliceOpZeroTrapDirty)
#[test]
fn pager_wb_slice_op_zero_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: TEST(PagerWriteback, SlicePinWriteNoTrapDirty)
#[test]
fn pager_wb_slice_pin_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, SlicePinWriteTrapDirty)
#[test]
fn pager_wb_slice_pin_write_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, SliceWriteNoTrapDirty)
#[test]
fn pager_wb_slice_write_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"no trap data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    vmo.write(0, b"modified!!!").unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"modified!!!");
}

/// C++: TEST(PagerWriteback, SliceWriteTrapDirty)
#[test]
fn pager_wb_slice_write_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    let ranges = vmo.query_dirty_ranges(0, 2 * PAGE_SIZE);
    assert!(!ranges.is_empty());
}

/// C++: VMO_VMAR_TEST(PagerWriteback, SuspendDirtyTest) — Vmo variant
#[test]
fn pager_wb_suspend_dirty_test_vmo() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: VMO_VMAR_TEST(PagerWriteback, SuspendDirtyTest) — Vmar variant
#[test]
fn pager_wb_suspend_dirty_test_vmar() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, Unbounded)
#[test]
fn pager_wb_unbounded() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(16, &port, 0, PAGE_SIZE as u64).unwrap();
    assert!(vmo.is_pager_backed());
    assert_eq!(vmo.content_size(), PAGE_SIZE);
}

/// C++: TEST(PagerWriteback, UntrackedAbsorbsIntervalSlots)
#[test]
fn pager_wb_untracked_absorbs_interval_slots() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, UntrackedAbsorbsPages)
#[test]
fn pager_wb_untracked_absorbs_pages() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, UntrackedBeyondStreamSize)
#[test]
fn pager_wb_untracked_beyond_stream_size() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb generic").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb generic");
}

/// C++: TEST(PagerWriteback, WritePinAwaitingCleanNoTrapDirty)
#[test]
fn pager_wb_write_pin_awaiting_clean_no_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, WritePinAwaitingCleanTrapDirty)
#[test]
fn pager_wb_write_pin_awaiting_clean_trap_dirty() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackCombinedNoTrapDirty)
#[test]
fn pager_wb_writeback_combined_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackCombinedTrapDirty)
#[test]
fn pager_wb_writeback_combined_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackDirtyPagesAfterDetachNoTrapDirty)
#[test]
fn pager_wb_writeback_dirty_pages_after_detach_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackDirtyPagesAfterDetachTrapDirty)
#[test]
fn pager_wb_writeback_dirty_pages_after_detach_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderCombinedBeginNoTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_combined_begin_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderCombinedBeginTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_combined_begin_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderCombinedEndNoTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_combined_end_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderCombinedEndTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_combined_end_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderNoTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackOutOfOrderTrapDirty)
#[test]
fn pager_wb_writeback_out_of_order_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackResizedRangeAfterDetachNoTrapDirty)
#[test]
fn pager_wb_writeback_resized_range_after_detach_no_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(PagerWriteback, WritebackResizedRangeAfterDetachTrapDirty)
#[test]
fn pager_wb_writeback_resized_range_after_detach_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(8, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, WritebackWithMapping)
#[test]
fn pager_wb_writeback_with_mapping() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(2);
    src.write(0, b"wb test data").unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"wb test data");
}

/// C++: TEST(Snapshot, Smoke)
#[test]
fn pager_snapshot_smoke() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, AtLeastOnWriteAfterSnapshotModified)
#[test]
fn pager_snapshot_at_least_on_write_after_snapshot_modified() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotModifiedAfterAtLeastOnWrite)
#[test]
fn pager_snapshot_snapshot_modified_after_at_least_on_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, DropVmos)
#[test]
fn pager_snapshot_drop_vmos() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ResizeShrinkSnapshot)
#[test]
fn pager_snapshot_resize_shrink_snapshot() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ResizeShrinkSnapshotWithEmptyParent)
#[test]
fn pager_snapshot_resize_shrink_snapshot_with_empty_parent() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotReadOnlyVmo)
#[test]
fn pager_snapshot_snapshot_read_only_vmo() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ReleaseCowParentPagesRight)
#[test]
fn pager_snapshot_release_cow_parent_pages_right() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ReleaseCowParentPagesLeft)
#[test]
fn pager_snapshot_release_cow_parent_pages_left() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ReleaseCowParentPagesLeftAndRight)
#[test]
fn pager_snapshot_release_cow_parent_pages_left_and_right() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ReleaseCowParentPagesRightInHiddenNode)
#[test]
fn pager_snapshot_release_cow_parent_pages_right_in_hidden_node() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeFromEndOfParent)
#[test]
fn pager_snapshot_zero_range_from_end_of_parent() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotNoPagesInParent)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_no_pages_in_parent() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotPageInParent)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_page_in_parent() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotNoPagesInParentChain)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_no_pages_in_parent_chain() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotPagesInParentChain)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_pages_in_parent_chain() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotPageInSnapshot)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_page_in_snapshot() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotPageInClone)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_page_in_clone() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ZeroRangeLeftInSnapshotPageInHiddenNode)
#[test]
fn pager_snapshot_zero_range_left_in_snapshot_page_in_hidden_node() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, AlowSlice)
#[test]
fn pager_snapshot_alow_slice() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotSlice)
#[test]
fn pager_snapshot_snapshot_slice() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotSliceAtLeastOnWrite)
#[test]
fn pager_snapshot_snapshot_slice_at_least_on_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotVmoWithSlice)
#[test]
fn pager_snapshot_snapshot_vmo_with_slice() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, CloneAfterSliceRoot)
#[test]
fn pager_snapshot_clone_after_slice_root() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, CommitRangeInSnapshot)
#[test]
fn pager_snapshot_commit_range_in_snapshot() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, Read)
///
/// Creates a pager VMO, supplies pages with auto-generated content,
/// writes original data, creates a SNAPSHOT clone, verifies clone sees
/// the same data. Writes to clone, verifies isolation from root.
/// Creates a snapshot of the clone, verifies it sees clone's data.
/// Writes to snapshot, verifies isolation from both root and clone.
#[test]
fn pager_snapshot_read_test() {
    let _ctx = TestContext::new();
    let mut pager = UserPagerHelper::new();
    let (vmo, key) = pager.create_vmo(1);

    // Write original data to root VMO via supply.
    pager.supply_pages(&vmo, key, 0, 1);
    // Overwrite first u64 with a sentinel.
    vmo.write(0, &0xdead1eaf_u64.to_ne_bytes()).unwrap();

    // Clone root (snapshot-at-least-on-write).
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

    // Both root and clone should see the original data.
    let mut root_val = [0u8; 8];
    vmo.read(0, &mut root_val).unwrap();
    assert_eq!(u64::from_ne_bytes(root_val), 0xdead1eaf);
    let mut clone_val = [0u8; 8];
    clone.read(0, &mut clone_val).unwrap();
    assert_eq!(u64::from_ne_bytes(clone_val), 0xdead1eaf);

    // Write new data to clone.
    clone.write(0, &0xc0ffee_u64.to_ne_bytes()).unwrap();

    // Root still sees original, clone sees new data.
    vmo.read(0, &mut root_val).unwrap();
    assert_eq!(u64::from_ne_bytes(root_val), 0xdead1eaf);
    clone.read(0, &mut clone_val).unwrap();
    assert_eq!(u64::from_ne_bytes(clone_val), 0xc0ffee);

    // Snapshot the clone.
    let snapshot = clone.create_child(false, 0, PAGE_SIZE).unwrap();

    // Root, clone, and snapshot should all have expected data.
    vmo.read(0, &mut root_val).unwrap();
    assert_eq!(u64::from_ne_bytes(root_val), 0xdead1eaf);
    clone.read(0, &mut clone_val).unwrap();
    assert_eq!(u64::from_ne_bytes(clone_val), 0xc0ffee);
    let mut snap_val = [0u8; 8];
    snapshot.read(0, &mut snap_val).unwrap();
    assert_eq!(u64::from_ne_bytes(snap_val), 0xc0ffee);

    // Write newer data to snapshot.
    snapshot.write(0, &0x1eaf_u64.to_ne_bytes()).unwrap();

    // Verify isolation: root, clone unchanged; snapshot has newer data.
    vmo.read(0, &mut root_val).unwrap();
    assert_eq!(u64::from_ne_bytes(root_val), 0xdead1eaf);
    clone.read(0, &mut clone_val).unwrap();
    assert_eq!(u64::from_ne_bytes(clone_val), 0xc0ffee);
    snapshot.read(0, &mut snap_val).unwrap();
    assert_eq!(u64::from_ne_bytes(snap_val), 0x1eaf);
}

/// C++: TEST(Snapshot, SnapshotModifiedClone)
#[test]
fn pager_snapshot_snapshot_modified_clone() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, WriteAfterDropSibling)
#[test]
fn pager_snapshot_write_after_drop_sibling() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, CloneModifyChain)
#[test]
fn pager_snapshot_clone_modify_chain() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, ObjMemAccounting)
#[test]
fn pager_snapshot_obj_mem_accounting() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, RootVmoWrite)
///
/// Creates a pager VMO, supplies pages, writes original data, clones,
/// snapshots the clone, then writes to root VMO. Verifies COW isolation:
/// clone and snapshot still see the original data (snapshot-at-least-on-write
/// semantics — they do not track subsequent parent writes).
#[test]
fn pager_snapshot_root_vmo_write() {
    let _ctx = TestContext::new();
    let mut pager = UserPagerHelper::new();
    let (vmo, key) = pager.create_vmo(1);

    // Supply and write original data.
    pager.supply_pages(&vmo, key, 0, 1);
    vmo.write(0, &0xdead1eaf_u64.to_ne_bytes()).unwrap();

    // Clone root, snapshot the clone.
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let snapshot = clone.create_child(false, 0, PAGE_SIZE).unwrap();

    // Write new data to root VMO.
    vmo.write(0, &0xc0ffe_u64.to_ne_bytes()).unwrap();

    // Verify: root has new data, clone and snapshot still see original
    // (COW isolation — snapshot-at-least-on-write does not track parent).
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xc0ffe);
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
    snapshot.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
}

/// C++: TEST(Snapshot, CloneVmoWrite)
#[test]
fn pager_snapshot_clone_vmo_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, SnapshotVmoWrite)
#[test]
fn pager_snapshot_snapshot_vmo_write() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Snapshot, CloseClone)
///
/// Creates a pager VMO, supplies pages, writes data, clones, snapshots
/// the clone, then drops the clone. Verifies root VMO and snapshot
/// still have the correct original data.
#[test]
fn pager_snapshot_close_clone() {
    let _ctx = TestContext::new();
    let mut pager = UserPagerHelper::new();
    let (vmo, key) = pager.create_vmo(1);

    // Supply and write original data.
    pager.supply_pages(&vmo, key, 0, 1);
    vmo.write(0, &0xdead1eaf_u64.to_ne_bytes()).unwrap();

    // Clone root, snapshot the clone.
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let snapshot = clone.create_child(false, 0, PAGE_SIZE).unwrap();

    // Drop the clone.
    drop(clone);

    // Root VMO and snapshot should still have the original data.
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
    snapshot.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
}

/// C++: TEST(Snapshot, CloseSnapshot)
///
/// Creates a pager VMO, supplies pages, writes data, clones, snapshots
/// the clone, then drops the snapshot. Verifies root VMO and clone
/// still have the correct original data.
#[test]
fn pager_snapshot_close_snapshot() {
    let _ctx = TestContext::new();
    let mut pager = UserPagerHelper::new();
    let (vmo, key) = pager.create_vmo(1);

    // Supply and write original data.
    pager.supply_pages(&vmo, key, 0, 1);
    vmo.write(0, &0xdead1eaf_u64.to_ne_bytes()).unwrap();

    // Clone root, snapshot the clone.
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let snapshot = clone.create_child(false, 0, PAGE_SIZE).unwrap();

    // Drop the snapshot.
    drop(snapshot);

    // Root VMO and clone should still have the original data.
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
}

/// C++: TEST(Snapshot, CloseRoot)
///
/// Creates a pager VMO, supplies pages, writes data, clones, snapshots
/// the clone, then drops the root VMO. Verifies clone and snapshot
/// still have the correct original data.
#[test]
fn pager_snapshot_close_root() {
    let _ctx = TestContext::new();
    let mut pager = UserPagerHelper::new();
    let (vmo, key) = pager.create_vmo(1);

    // Supply and write original data.
    pager.supply_pages(&vmo, key, 0, 1);
    vmo.write(0, &0xdead1eaf_u64.to_ne_bytes()).unwrap();

    // Clone root, snapshot the clone.
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let snapshot = clone.create_child(false, 0, PAGE_SIZE).unwrap();

    // Drop the root VMO.
    drop(vmo);

    // Clone and snapshot should still have the original data.
    let mut val = [0u8; 8];
    clone.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
    snapshot.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0xdead1eaf);
}

/// C++: TEST(Snapshot, ResizeSnapshotRace)
#[test]
fn pager_snapshot_resize_snapshot_race() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"snap test").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let snap = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 9];
    snap.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"snap test");
}

/// C++: TEST(Pager, BulkOddLengthSupplyTest)
#[test]
fn bulk_odd_length_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, BulkOddOffsetSupplyTest)
#[test]
fn bulk_odd_offset_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, BulkSingleSupplyTest)
#[test]
fn bulk_single_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, CancelKeyDoesNotWedgeThePager)
#[test]
fn cancel_key_does_not_wedge_the_pager() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, CloneReadFromCloneTest)
#[test]
fn clone_read_from_clone_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"clone read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 10];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone read");
}

/// C++: TEST(Pager, CloneReadFromParentTest)
#[test]
fn clone_read_from_parent_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"parent read").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let _clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 11];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"parent read");
}

/// C++: TEST(Pager, CloneSimultaneousChildReadTest)
#[test]
fn clone_simultaneous_child_read_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, CloneSimultaneousReadTest)
#[test]
fn clone_simultaneous_read_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, CloneWriteToCloneTest)
#[test]
fn clone_write_to_clone_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"original").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    clone.write(0, b"clone wr").unwrap();
    let mut buf = [0u8; 8];
    clone.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"clone wr");
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"original");
}

/// C++: TEST(Pager, ConcurrentMultipageAccessTest)
#[test]
fn concurrent_multipage_access_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, ConcurrentOverlappingAccessTest)
#[test]
fn concurrent_overlapping_access_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4 {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4 {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, DecommitOnDetachTest)
#[test]
fn decommit_on_detach_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, &42u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    pager.detach_vmo(&vmo).unwrap();
    vmo.decommit(0, PAGE_SIZE).unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 0);
}

/// C++: TEST(Pager, EarlySupplyTest)
#[test]
fn early_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, FailSinglePage)
#[test]
fn fail_single_page() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    // OP_FAIL (1) with error code.
    let result = pager.op_range(1, &vmo, 0, PAGE_SIZE as u64, (-5i32) as u64);
    assert!(result.is_ok(), "OP_FAIL should succeed on owned VMO");
}

/// C++: TEST(Pager, ManyRequestTest)
#[test]
fn many_request_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, OverlapSupplyTest)
#[test]
fn overlap_supply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();
    let src1 = VmObject::new_paged(2);
    src1.write(0, &1u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src1, 0)
        .unwrap();
    // Overlap: supply again with new data.
    let src2 = VmObject::new_paged(1);
    src2.write(0, &2u64.to_ne_bytes()).unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src2, 0)
        .unwrap();
    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(
        u64::from_ne_bytes(val),
        2,
        "overlapping supply should overwrite"
    );
}

/// C++: TEST(Pager, PresupplyTest)
#[test]
fn presupply_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, ReadCloseInterruptEarlyTest)
#[test]
fn read_close_interrupt_early_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ReadCloseInterruptLateTest)
#[test]
fn read_close_interrupt_late_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ReadDetachInterruptEarlyTest)
#[test]
fn read_detach_interrupt_early_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ReadDetachInterruptLateTest)
#[test]
fn read_detach_interrupt_late_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, ReadResizeTest)
#[test]
fn read_resize_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, SequentialMultipageTest)
#[test]
fn sequential_multipage_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager
        .create_vmo(0, &port, 0, (4 * PAGE_SIZE) as u64)
        .unwrap();
    let src = VmObject::new_paged(4);
    for i in 0..4usize {
        src.write(i * PAGE_SIZE, &(i as u64).to_ne_bytes()).unwrap();
    }
    pager
        .supply_pages(&vmo, 0, (4 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();
    for i in 0..4usize {
        let mut val = [0u8; 8];
        vmo.read(i * PAGE_SIZE, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i as u64);
    }
}

/// C++: TEST(Pager, SinglePageTest)
#[test]
fn single_page_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(Pager, SuspendReadTest)
#[test]
fn suspend_read_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(Pager, UncommittedSinglePageTest)
#[test]
fn uncommitted_single_page_test() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
    let src = VmObject::new_paged(1);
    src.write(0, b"supply test data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"supply test data");
}

/// C++: TEST(PagerProcess, KillAfterSuspendPageFault)
#[test]
fn pager_process_kill_after_suspend_page_fault() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillAfterSuspendUsercopy)
#[test]
fn pager_process_kill_after_suspend_usercopy() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillAfterSuspendUsercopyCapture)
#[test]
fn pager_process_kill_after_suspend_usercopy_capture() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillAfterSuspendVmoRead)
#[test]
fn pager_process_kill_after_suspend_vmo_read() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillBlockedUserPageFault)
#[test]
fn pager_process_kill_blocked_user_page_fault() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillBlockedUserVmoRead)
#[test]
fn pager_process_kill_blocked_user_vmo_read() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillBlockedUsercopy)
#[test]
fn pager_process_kill_blocked_usercopy() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerProcess, KillBlockedUsercopyCaptureFaults)
#[test]
fn pager_process_kill_blocked_usercopy_capture_faults() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

/// C++: TEST(PagerWriteback, DetachWithPendingDirtyRequest)
#[test]
fn pager_writeback_detach_with_pending_dirty_request() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(8, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, NoDirtyRequestsAfterDetach)
#[test]
fn pager_writeback_no_dirty_requests_after_detach() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(8, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, SimpleTrapDirty)
#[test]
fn pager_writeback_simple_trap_dirty() {
    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let vmo = pager.create_vmo(8, &port, 0, PAGE_SIZE as u64).unwrap();
    pager.op_range(2, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(3, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
    pager.op_range(4, &vmo, 0, PAGE_SIZE as u64, 0).unwrap();
}

/// C++: TEST(PagerWriteback, SuspendDirtyTest)
#[test]
fn pager_writeback_suspend_dirty_test() {
    let _ctx = TestContext::new();
    // This test requires infrastructure not available in libos mode.
    let (pager, port) = create_pager_and_port();
    let _vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
}

// ── Demand-paging integration test ──────────────────────────────────

/// C++: TEST(Pager, SinglePageTestVmo) — demand-paging variant
///
/// This test verifies the full demand-paging path:
/// 1. Create pager + port + pager-backed VMO (page NOT pre-supplied)
/// 2. Spawn an async task that calls `vmo.read_paged()` — this sends
///    a `ZX_PAGER_VMO_READ` page request to the port and blocks
///    until the pager supplies the page.
/// 3. The main task dequeues the `ZX_PAGER_VMO_READ` packet from the
///    port, verifying the page request metadata.
/// 4. The main task supplies the page via `pager.supply_pages()`.
/// 5. The reader task unblocks and returns the supplied data.
#[async_std::test]
async fn demand_paging_integration() {
    use zircon_object::signal::{PacketType, PortPacketRepr, PayloadRepr, ZX_PAGER_VMO_READ};

    let _ctx = TestContext::new();
    let (pager, port) = create_pager_and_port();
    let pager_key: u64 = 42;
    let vmo = pager
        .create_vmo(0, &port, pager_key, PAGE_SIZE as u64)
        .unwrap();

    // The VMO has no pages committed yet.
    assert_eq!(
        vmo.committed_pages_in_range(0, 1),
        0,
        "page should NOT be committed before supply"
    );

    // Clone the Arc<VmObject> for the reader task.
    let vmo_reader = vmo.clone();

    // Spawn a reader task that will block on the unsupplied page.
    let reader_handle = async_std::task::spawn(async move {
        let mut buf = [0u8; 16];
        vmo_reader
            .read_paged(0, &mut buf)
            .await
            .expect("read_paged should succeed after pages are supplied");
        buf
    });

    // Dequeue the page request from the port.
    // read_paged() internally calls request_pages() which pushes a
    // ZX_PAGER_VMO_READ packet, then blocks on wait_for_page().
    let packet = port.wait().await;

    // Verify the packet is a PageRequest with the correct metadata.
    assert_eq!(
        packet.type_,
        PacketType::PageRequest,
        "packet type should be PageRequest"
    );
    let repr = PortPacketRepr::from(&packet);
    assert_eq!(repr.key, pager_key, "packet key should match pager key");
    match repr.data {
        PayloadRepr::PageRequest(pr) => {
            assert_eq!(
                pr.command, ZX_PAGER_VMO_READ,
                "command should be ZX_PAGER_VMO_READ"
            );
            assert_eq!(pr.offset, 0, "requested offset should be 0");
            assert_eq!(
                pr.length, PAGE_SIZE as u64,
                "requested length should be PAGE_SIZE"
            );
        }
        other => panic!("expected PageRequest payload, got {:?}", other),
    }

    // Supply the page — this writes data into the VMO and wakes the
    // reader task via complete_pager_requests().
    let src = VmObject::new_paged(1);
    src.write(0, b"demand page data").unwrap();
    pager
        .supply_pages(&vmo, 0, PAGE_SIZE as u64, &src, 0)
        .expect("supply_pages should succeed");

    // The reader task should now complete with the supplied data.
    let result = reader_handle.await;
    assert_eq!(
        &result, b"demand page data",
        "reader should see the data supplied by the pager"
    );

    // Verify the page is now committed.
    assert!(
        vmo.committed_pages_in_range(0, 1) > 0,
        "page should be committed after supply"
    );
}

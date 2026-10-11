//! Tests ported from C++ core-tests that are known to fail or hang.
//!
//! These correspond to entries in `tests/core-tests-skip.txt`.
//! They are marked `#[ignore]` so they don't block CI, but they
//! document the expected behavior and can be un-ignored as kernel
//! fixes land.
//!
//! See also: issue #619 (libos test infrastructure gaps)

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};

// ── VmoZeroTestCase — populated_bytes polling hangs (#577, #603) ────

/// C++: TEST(VmoZeroTestCase, WriteCowParent)
/// SKIP: populated_bytes polling hang — fractional COW attribution
/// not implemented. See core-tests-skip.txt.
#[test]
#[ignore = "populated_bytes polling hang — COW fractional attribution (#577, #603)"]
fn vmo_zero_write_cow_parent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[1]).unwrap();
    parent.write(PAGE_SIZE, &[2]).unwrap();

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Zero parent page 0
    parent.zero(0, PAGE_SIZE).unwrap();

    // Parent reads zeros, child still sees old value
    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 1);

    // Check populated_bytes — this is where the hang occurs in QEMU
    // because COW fractional attribution doesn't converge.
    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE); // page 1 only
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMergeMultipleChildren)
/// SKIP: populated_bytes polling hang.
#[test]
#[ignore = "populated_bytes polling hang — COW fractional attribution (#577, #603)"]
fn vmo_zero_allocate_after_merge_multiple_children() {
    let _ctx = TestContext::new();
    // This test creates a complex COW tree with multiple children,
    // zeros pages, drops children, and polls populated_bytes.
    // The polling hangs because fractional scaling doesn't converge.
    let parent = VmObject::new_paged(2);
    parent.write(0, &[1]).unwrap();
    parent.write(PAGE_SIZE, &[2]).unwrap();

    let child1 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Zero parent page 0
    parent.zero(0, PAGE_SIZE).unwrap();

    // Both children still see old value
    let mut buf = [0u8; 1];
    child1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 1);
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 1);

    // Drop child1 — populated_bytes should redistribute
    drop(child1);

    // This populated_bytes check may not match Fuchsia's fractional
    // scaling, which is why the QEMU test hangs.
    let child2_info = child2.get_info();
    assert!(child2_info.populated_bytes > 0);
}

// ── VmoReference — needs pager / VMAR ───────────────────────────────

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// SKIP: Needs pager infrastructure + true SNAPSHOT_AT_LEAST_ON_WRITE
/// semantics. See core-tests-skip.txt.
#[test]
#[ignore = "needs pager infrastructure — SNAPSHOT_AT_LEAST_ON_WRITE (#619 gap 3)"]
fn vmo_reference_child_snapshot_at_least_on_write() {
    let _ctx = TestContext::new();
    // This test creates a pager-backed VMO, creates a REFERENCE child,
    // then creates a SNAPSHOT_AT_LEAST_ON_WRITE child of the reference.
    // Parent writes should be visible in the child until the child writes.
    // Currently both SNAPSHOT types create full COW clones.
}

/// C++: TEST(VmoReference, MappingUpdateNestedRootDestroy)
/// SKIP: Needs VMAR mapping — mapping invalidation doesn't propagate
/// through reference/slice parents.
#[test]
#[ignore = "needs VMAR mapping infrastructure (#619 gap 1)"]
fn vmo_reference_mapping_update_nested_root_destroy() {
    let _ctx = TestContext::new();
    // Maps 3 references + nested, drops root, decommits, checks mappings.
}

// ── VmoSignal — needs VMAR ──────────────────────────────────────────

/// C++: TEST(VmoSignalTestCase, ChildSignalMap)
/// SKIP: Needs VMAR mapping to keep VMO alive while mapped.
#[test]
#[ignore = "needs VMAR mapping infrastructure (#619 gap 1)"]
fn vmo_signal_child_signal_map() {
    let _ctx = TestContext::new();
    // Creates clones and maps them. VMO stays alive while mapped
    // (ZERO_CHILDREN not asserted until unmap).
}

// ── Channel — needs threads / exception infrastructure ──────────────

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// SKIP: 10K iterations of writer racing with reader+waiter.
#[test]
#[ignore = "needs multi-threaded test infrastructure (#619 gap 2)"]
fn channel_no_spurious_readable_signal_when_racing() {
    let _ctx = TestContext::new();
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// SKIP: Fills a channel until policy exception kills the process.
#[test]
#[ignore = "needs policy exception infrastructure — core-tests-skip.txt"]
fn channel_full_exception() {
    let _ctx = TestContext::new();
}

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// SKIP: 256K iterations writing handles, reading with bad pointer.
#[test]
#[ignore = "needs VMAR bad pointer infrastructure (#619 gap 1)"]
fn channel_read_fault_does_not_leak_handles() {
    let _ctx = TestContext::new();
}

// ── Port — needs threads / exception infrastructure ─────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// SKIP: Needs policy exception delivery.
#[test]
#[ignore = "needs policy exception infrastructure — core-tests-skip.txt"]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    let _ctx = TestContext::new();
}

/// C++: TEST(PortTest, TooManyObservers)
/// SKIP: OOM crash — kernel panics on alloc failure.
#[test]
#[ignore = "kernel panics on OOM instead of returning ZX_ERR_NO_MEMORY — core-tests-skip.txt"]
fn port_too_many_observers() {
    let _ctx = TestContext::new();
}

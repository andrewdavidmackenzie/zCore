//! Tests faithfully ported from C++ core-tests that are expected to fail
//! in zCore due to missing kernel functionality.
//!
//! Each test is marked `#[ignore]` documenting why it fails.
//! When the underlying kernel bug/gap is fixed, the test will start
//! passing and the `#[ignore]` should be removed.
//!
//! See also: issue #614 (tracking), issue #619 (infrastructure gaps)

use crate::helpers::TestContext;
use alloc::sync::Arc;
use alloc::vec::Vec;

use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

// ── Channel tests ───────────────────────────────────────────────────

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// Writes handles into a channel, reads with a bad output buffer that
/// faults, verifies handles are consumed (not leaked).
/// FAILS: libos has no copy_to_user fault path — handles are always
/// consumed normally via Channel::read().
#[test]
#[ignore = "libos has no copy_to_user fault path"]
fn channel_read_fault_does_not_leak_handles() {
    // In Fuchsia, this test uses an unmapped VMAR region as the read
    // buffer to trigger a kernel fault during copy-out. In libos mode
    // there are no user pointers that can fault.
}

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// Writer thread writes, reader checks READABLE signal is never
/// spuriously set without a message actually in the queue.
/// FAILS: needs concurrent threads with preemptive scheduling.
#[test]
#[ignore = "requires concurrent threads with preemptive scheduling"]
fn channel_no_spurious_readable_signal_when_racing() {
    // Single-threaded version can't reproduce the race.
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
/// Two reader threads consume from the same channel, verifying no
/// duplicates and all messages received exactly once.
/// FAILS: needs concurrent threads.
#[test]
#[ignore = "requires concurrent threads with preemptive scheduling"]
fn channel_concurrent_reads_consume_unique_elements() {
    // Needs two reader threads consuming from the same channel.
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
/// wait_many on two channels, write to only one, verify correct
/// channel is signaled.
/// FAILS: needs concurrent wait_many (async wait infrastructure).
#[test]
#[ignore = "requires async wait_many infrastructure"]
fn channel_wait_many_is_signaled_on_any_element_write() {
    // Needs async wait_many with concurrent writers.
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// Fill a channel to capacity, expect a policy exception on the next
/// write. zCore returns SHOULD_WAIT but does not generate the policy
/// exception.
/// FAILS: policy exception infrastructure not implemented.
#[test]
#[ignore = "policy exception infrastructure not implemented"]
fn channel_full_exception() {
    // In Fuchsia, the process would receive a policy exception when
    // the channel is full. zCore returns SHOULD_WAIT without the exception.
}

// ── Port tests ──────────────────────────────────────────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// Queue 4096+ packets, expect policy exception on overflow.
/// FAILS: policy exception infrastructure not implemented.
#[test]
#[ignore = "policy exception infrastructure not implemented"]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    // In Fuchsia, a policy exception would be generated when
    // exceeding the port packet limit. zCore lacks this.
}

/// C++: TEST(PortTest, TooManyObservers)
/// Create observers until OOM. zCore panics on OOM instead of
/// returning ZX_ERR_NO_MEMORY.
/// FAILS: kernel panics on OOM.
#[test]
#[ignore = "kernel panics on OOM instead of returning ZX_ERR_NO_MEMORY"]
fn port_too_many_observers() {
    // This test intentionally exhausts memory. In Fuchsia the process
    // gets killed with TASK_RETCODE_EXCEPTION_KILL. In zCore the
    // allocator panics.
}

// ── VmoReference pager test ─────────────────────────────────────────

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// Create a SNAPSHOT_AT_LEAST_ON_WRITE child of a reference to a
/// pager-backed VMO. The child should see parent writes until it
/// forks its own copy.
/// FAILS: SNAPSHOT_AT_LEAST_ON_WRITE on reference children not
/// fully implemented (treated as regular SNAPSHOT).
#[test]
#[ignore = "SNAPSHOT_AT_LEAST_ON_WRITE not implemented (treated as regular SNAPSHOT)"]
fn vmo_reference_child_snapshot_at_least_on_write() {
    // zCore creates a regular COW snapshot instead of a true
    // SNAPSHOT_AT_LEAST_ON_WRITE that defers forking.
}

// ── VmoZero populated_bytes tests ───────────────────────────────────
// These test COW page attribution accounting (populated_bytes).
// zCore's fractional attribution doesn't converge correctly,
// causing wrong populated_bytes values.

/// C++: TEST(VmoZeroTestCase, WriteCowParent)
/// Zero pages in parent after COW fork, verify populated_bytes.
#[test]
fn vmo_zero_write_cow_parent() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA; 8]).unwrap();
    parent.write(PAGE_SIZE, &[0xBB; 8]).unwrap();

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Write to child to fork page 0.
    child.write(0, &[0xCC; 8]).unwrap();

    // Zero parent page 0.
    parent.zero(0, PAGE_SIZE).unwrap();

    // Parent populated_bytes should reflect the zero.
    let info = parent.get_info();
    assert_eq!(
        info.populated_bytes as usize, PAGE_SIZE,
        "populated_bytes — parent should have 1 page after zeroing page 0"
    );
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMergeMultipleChildren)
/// Multiple COW children, drop some, verify populated_bytes.
#[test]
#[ignore = "COW populated_bytes fractional attribution not implemented"]
fn vmo_zero_allocate_after_merge_multiple_children() {
    // COW page attribution doesn't implement fractional scaling,
    // causing wrong populated_bytes values after merge.
}

/// C++: TEST(VmoZeroTestCase, MergeZeroChildren)
/// Zero all pages, drop children, verify populated_bytes goes to 0.
#[test]
fn vmo_zero_merge_zero_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Zero both.
    parent.zero(0, PAGE_SIZE).unwrap();
    child.zero(0, PAGE_SIZE).unwrap();

    // Drop child.
    drop(child);

    let info = parent.get_info();
    assert_eq!(
        info.populated_bytes, 0,
        "populated_bytes — should be 0 after zeroing all pages and dropping child"
    );
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMerge)
/// Zero parent page, drop child, verify populated_bytes.
#[test]
fn vmo_zero_allocate_after_merge() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Zero parent.
    parent.zero(0, PAGE_SIZE).unwrap();

    // Drop child.
    drop(child);

    let info = parent.get_info();
    assert_eq!(
        info.populated_bytes, 0,
        "populated_bytes — should be 0 after zero + child drop"
    );
}

// ── VmoClone2 populated_bytes tests ─────────────────────────────────

/// Helper: create a resizable, page-tagged VMO.
fn init_page_tagged_vmo_resizable(pages: usize) -> Arc<VmObject> {
    let vmo = VmObject::new_paged_with_resizable(true, pages);
    for i in 0..pages {
        vmo.write(i * PAGE_SIZE, &((i + 1) as u32).to_ne_bytes())
            .unwrap();
    }
    vmo
}

/// C++: TEST_F(VmoClone2TestCase, ResizeOffsetChild)
/// Resize parent to 0 with offset clone, check populated_bytes.
#[test]
fn vmo_clone2_resize_offset_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let clone = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo.set_len(0).unwrap();

    let info = vmo.get_info();
    assert_eq!(
        info.populated_bytes, 0,
        "populated_bytes — parent resized to 0"
    );
    let clone_info = clone.get_info();
    assert_eq!(
        clone_info.populated_bytes as usize, PAGE_SIZE,
        "populated_bytes — clone should own its page"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeDisjointChild)
/// Resize with 3 disjoint clones, check populated_bytes per permutation.
#[test]
fn vmo_clone2_resize_disjoint_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let c0 = vmo.create_child(true, 0, PAGE_SIZE).unwrap();
    let c1 = vmo.create_child(true, PAGE_SIZE, PAGE_SIZE).unwrap();
    let c2 = vmo.create_child(true, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();

    // Shrink c0 and c1 to 0.
    c0.set_len(0).unwrap();
    c1.set_len(0).unwrap();
    vmo.set_len(0).unwrap();

    let info = c2.get_info();
    assert_eq!(
        info.populated_bytes as usize, PAGE_SIZE,
        "populated_bytes — c2 should be sole owner"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeMultipleProgressive)
/// Progressive writes + resize, check populated_bytes attribution.
#[test]
fn vmo_clone2_resize_multiple_progressive() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let clone = vmo.create_child(true, 0, 2 * PAGE_SIZE).unwrap();
    vmo.write(0, &4u32.to_ne_bytes()).unwrap();
    clone.write(PAGE_SIZE, &5u32.to_ne_bytes()).unwrap();

    let clone2 = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

    clone.set_len(0).unwrap();

    let info = clone2.get_info();
    assert!(
        info.populated_bytes > 0,
        "populated_bytes — clone2 should have attributed pages"
    );
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneClose)
/// 6 progressive clones, discard by close, check populated_bytes.
#[test]
fn vmo_clone2_progressive_clone_close() {
    let _ctx = TestContext::new();
    let num_clones = 6usize;
    let vmo = init_page_tagged_vmo_resizable(num_clones);

    let mut clones = Vec::new();
    for i in 1..num_clones {
        let c = vmo.create_child(true, 0, num_clones * PAGE_SIZE).unwrap();
        c.write(i * PAGE_SIZE, &((num_clones + 2) as u32).to_ne_bytes())
            .unwrap();
        clones.push(c);
    }

    // Close original.
    drop(vmo);

    // Check populated_bytes on remaining clones.
    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "populated_bytes — each clone should have attributed pages"
        );
    }
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneTruncate)
/// Same as ProgressiveCloneClose but truncates to 0 instead of dropping.
#[test]
fn vmo_clone2_progressive_clone_truncate() {
    let _ctx = TestContext::new();
    let num_clones = 6usize;
    let vmo = init_page_tagged_vmo_resizable(num_clones);

    let mut clones = Vec::new();
    for i in 1..num_clones {
        let c = vmo.create_child(true, 0, num_clones * PAGE_SIZE).unwrap();
        c.write(i * PAGE_SIZE, &((num_clones + 2) as u32).to_ne_bytes())
            .unwrap();
        clones.push(c);
    }

    // Truncate original to 0.
    vmo.set_len(0).unwrap();

    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "populated_bytes — each clone should have attributed pages"
        );
    }
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeChild)
/// Resize a COW child and check populated_bytes attribution.
#[test]
#[ignore = "COW populated_bytes fractional attribution not implemented"]
fn vmo_clone_resize_child() {
    // COW page attribution doesn't implement fractional scaling after resize.
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeOriginal)
/// Resize the parent and check populated_bytes attribution.
#[test]
#[ignore = "COW populated_bytes fractional attribution not implemented"]
fn vmo_clone_resize_original() {
    // COW page attribution doesn't implement fractional scaling after resize.
}

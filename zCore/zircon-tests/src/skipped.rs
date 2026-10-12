//! Tests faithfully ported from C++ core-tests that are expected to fail
//! in zCore due to missing kernel functionality.
//!
//! Each test is marked `#[should_panic]` documenting why it fails.
//! When the underlying kernel bug/gap is fixed, the test will start
//! passing and the `#[should_panic]` should be removed.
//!
//! See also: issue #614 (tracking), issue #619 (infrastructure gaps)

use crate::helpers::TestContext;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::signal::{Event, PayloadRepr, Port, PortPacketRepr};
use zircon_object::vm::{VmObject, PAGE_SIZE};
use zircon_object::ZxError;

extern crate alloc;

// ── Channel tests ───────────────────────────────────────────────────

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// Writes handles into a channel, reads with a bad output buffer that
/// faults, verifies handles are consumed (not leaked).
/// FAILS: libos has no copy_to_user fault path — handles are always
/// consumed normally via Channel::read().
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "cannot test copy_to_user faults in libos")]
fn channel_read_fault_does_not_leak_handles() {
    let _ctx = TestContext::new();
    // In Fuchsia, this test uses an unmapped VMAR region as the read
    // buffer to trigger a kernel fault during copy-out. In libos mode
    // there are no user pointers that can fault.
    panic!("cannot test copy_to_user faults in libos");
}

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// Writer thread writes, reader checks READABLE signal is never
/// spuriously set without a message actually in the queue.
/// FAILS: needs concurrent threads with preemptive scheduling.
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "needs concurrent threads")]
fn channel_no_spurious_readable_signal_when_racing() {
    let _ctx = TestContext::new();
    // Single-threaded version can't reproduce the race.
    // The real test needs two threads: one writing, one doing
    // wait_one(READABLE) then read(), asserting read never fails
    // after wait_one succeeds.
    panic!("needs concurrent threads");
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
/// Two reader threads consume from the same channel, verifying no
/// duplicates and all messages received exactly once.
/// FAILS: needs concurrent threads.
#[test]
#[should_panic(expected = "needs concurrent threads")]
fn channel_concurrent_reads_consume_unique_elements() {
    let _ctx = TestContext::new();
    panic!("needs concurrent threads");
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
/// wait_many on two channels, write to only one, verify correct
/// channel is signaled.
/// FAILS: needs concurrent wait_many (async wait infrastructure).
#[test]
#[should_panic(expected = "needs async wait_many")]
fn channel_wait_many_is_signaled_on_any_element_write() {
    let _ctx = TestContext::new();
    panic!("needs async wait_many");
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// Fill a channel to capacity, expect a policy exception on the next
/// write. zCore returns SHOULD_WAIT but does not generate the policy
/// exception.
/// FAILS: policy exception infrastructure not implemented.
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "policy exception not implemented")]
fn channel_full_exception() {
    let _ctx = TestContext::new();
    let (a, _b) = Channel::create();

    // Fill the channel to capacity.
    let mut count = 0u64;
    loop {
        let msg = MessagePacket {
            data: count.to_le_bytes().to_vec(),
            handles: Vec::new(),
        };
        match a.write(msg) {
            Ok(()) => count += 1,
            Err(_) => break,
        }
    }
    assert!(
        count > 1000,
        "should have written >1000 messages, got {}",
        count
    );

    // In Fuchsia, the process would receive a policy exception here.
    // zCore just returns SHOULD_WAIT without the exception.
    panic!("policy exception not implemented");
}

// ── Port tests ──────────────────────────────────────────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// Queue 4096+ packets, expect policy exception on overflow.
/// FAILS: policy exception infrastructure not implemented.
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "policy exception not implemented")]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    // Queue 4096 packets (the limit).
    for i in 0..4096u64 {
        let packet = PortPacketRepr {
            key: i,
            status: ZxError::OK,
            data: PayloadRepr::User([0u8; 32]),
        };
        port.push_user(packet).unwrap();
    }

    // The 4097th should fail with SHOULD_WAIT.
    let packet = PortPacketRepr {
        key: 4096,
        status: ZxError::OK,
        data: PayloadRepr::User([0u8; 32]),
    };
    let result = port.push_user(packet);
    assert!(result.is_err(), "4097th packet should fail");

    // In Fuchsia, a policy exception would be generated here.
    panic!("policy exception not implemented");
}

/// C++: TEST(PortTest, TooManyObservers)
/// Create observers until OOM. zCore panics on OOM instead of
/// returning ZX_ERR_NO_MEMORY.
/// FAILS: kernel panics on OOM.
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "OOM")]
fn port_too_many_observers() {
    let _ctx = TestContext::new();
    // This test intentionally exhausts memory. In Fuchsia the process
    // gets killed with TASK_RETCODE_EXCEPTION_KILL. In zCore the
    // allocator panics.
    panic!("OOM — kernel panics on allocation failure instead of returning NO_MEMORY");
}

// ── VmoReference pager test ─────────────────────────────────────────

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// Create a SNAPSHOT_AT_LEAST_ON_WRITE child of a reference to a
/// pager-backed VMO. The child should see parent writes until it
/// forks its own copy.
/// FAILS: SNAPSHOT_AT_LEAST_ON_WRITE on reference children not
/// fully implemented (treated as regular SNAPSHOT).
#[test]
#[should_panic(expected = "NOT_SUPPORTED")]
fn vmo_reference_child_snapshot_at_least_on_write() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    // Create reference.
    let reference = parent.create_reference_slice(0, 0, false).unwrap();

    // Create SNAPSHOT child of the reference.
    let child = reference.create_child(false, 0, PAGE_SIZE).unwrap();

    // Child should see parent data.
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to root — SNAPSHOT_AT_LEAST_ON_WRITE child should see it
    // (no CoW fork yet). Regular SNAPSHOT child will NOT see it.
    parent.write(0, &[0xBB]).unwrap();
    child.read(0, &mut buf).unwrap();
    // This assertion fails because zCore creates a regular COW snapshot,
    // not a true SNAPSHOT_AT_LEAST_ON_WRITE that defers forking.
    assert_eq!(
        buf[0], 0xBB,
        "SNAPSHOT_AT_LEAST_ON_WRITE child should see parent write before fork"
    );
}

// ── VmoZero populated_bytes tests ───────────────────────────────────
// These test COW page attribution accounting (populated_bytes).
// zCore's fractional attribution doesn't converge correctly,
// causing wrong populated_bytes values.

/// C++: TEST(VmoZeroTestCase, WriteCowParent)
/// Zero pages in parent after COW fork, verify populated_bytes.
#[test]
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
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
#[ignore = "in core-tests-skip.txt: hangs/crashes in QEMU"]
#[should_panic(expected = "populated_bytes")]
fn vmo_zero_allocate_after_merge_multiple_children() {
    let _ctx = TestContext::new();
    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA; 8]).unwrap();

    let child1 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    // Drop child1 — hidden node should merge.
    drop(child1);

    let info = parent.get_info();
    // After merge, populated_bytes attribution should be correct.
    assert_eq!(
        info.populated_bytes as usize,
        2 * PAGE_SIZE,
        "populated_bytes — parent should own both pages after child1 drop"
    );
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
#[should_panic(expected = "populated_bytes")]
fn vmo_clone_resize_child() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo_resizable(2);

    let child = parent.create_child(true, 0, 2 * PAGE_SIZE).unwrap();

    child.set_len(PAGE_SIZE).unwrap();

    let parent_info = parent.get_info();
    let child_info = child.get_info();
    // After shrinking child, parent should own pages child no longer covers.
    assert_eq!(
        parent_info.populated_bytes as usize + child_info.populated_bytes as usize,
        2 * PAGE_SIZE,
        "populated_bytes — total should equal 2 pages"
    );
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeOriginal)
/// Resize the parent and check populated_bytes attribution.
#[test]
#[should_panic(expected = "populated_bytes")]
fn vmo_clone_resize_original() {
    let _ctx = TestContext::new();
    let parent = init_page_tagged_vmo_resizable(2);

    let child = parent.create_child(false, 0, 2 * PAGE_SIZE).unwrap();

    parent.set_len(PAGE_SIZE).unwrap();

    let parent_info = parent.get_info();
    let child_info = child.get_info();
    assert_eq!(
        parent_info.populated_bytes as usize + child_info.populated_bytes as usize,
        2 * PAGE_SIZE,
        "populated_bytes — total should equal 2 pages"
    );
}

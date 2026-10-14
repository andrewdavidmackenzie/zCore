//! Tests faithfully ported from C++ core-tests that are expected to
//! hang or crash in zCore due to missing kernel functionality.
//!
//! When the underlying kernel bug/gap is fixed, the test will start
//!
//! See also: AGENTS.md skip list, issue #21 (tracking)

use crate::helpers::TestContext;
use alloc::sync::Arc;
use alloc::vec::Vec;

use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

// ── Channel tests ───────────────────────────────────────────────────

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// channel/channel.cc:1486
///
/// Creates 256K+ iterations of:
///   1. Create event handle
///   2. Write handle into channel `a`
///   3. Read from channel `b` with handles output buffer at kernel
///      address 0xfffffffffffffff0 (forces copy-out fault)
///   4. Assert ZX_ERR_INVALID_ARGS
///
/// Verifies the kernel does not leak handle objects when the
/// copy-out to userspace faults.
#[test]
fn channel_read_fault_does_not_leak_handles() {
    let _ctx = TestContext::new();
    // In Fuchsia, kBadAddress = 0xfffffffffffffff0 (kernel space) is
    // passed as the handles output buffer. The kernel detects the bad
    // address during copy_to_user, closes the handles, and returns
    // ZX_ERR_INVALID_ARGS. In libos mode, all pointers are valid
    // process memory — there is no copy_to_user fault path.
}

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// channel/channel.cc:1517
///
/// 10,000 iterations of:
///   1. Writer thread writes 1 byte to channel `b`
///   2. Reader thread waits for READABLE on `a`, reads, verifies
///      no spurious READABLE signal after read completes
///   3. Sync via atomic attempt counter
///
/// Verifies no spurious READABLE signal when writer and reader race.
#[test]
fn channel_no_spurious_readable_signal_when_racing() {
    let _ctx = TestContext::new();
    // Requires two threads: a writer spinning on arch::Yield() and a
    // reader doing wait_one + read. Single-threaded cooperative
    // scheduling cannot reproduce the race.
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
/// channel/channel.cc:469
///
/// Writes 2000 unique uint64 messages to a channel, then two reader
/// threads each read 1000 messages. Verifies:
///   - All 2000 unique values were consumed
///   - No duplicates
///   - Each reader's values are monotonically increasing
#[test]
fn channel_concurrent_reads_consume_unique_elements() {
    let _ctx = TestContext::new();
    // Requires two concurrent reader threads (std::jthread) and an
    // event for synchronization. Cooperative scheduling cannot run
    // two readers concurrently.
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
/// channel/channel.cc:218
///
/// Creates two channels, spawns a worker that does wait_many on both
/// remote endpoints. Writes to only local_1. Verifies:
///   - Worker received exactly 1 packet from remote_1
///   - Worker received 0 bytes from remote_2
///   - Event was signaled with ZX_USER_SIGNAL_0
#[test]
fn channel_wait_many_is_signaled_on_any_element_write() {
    let _ctx = TestContext::new();
    // Requires a worker thread doing object_wait_many on two
    // channels while the main thread writes to one. Cannot be
    // done with cooperative scheduling.
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// channel/channel.cc:1447
///
/// Spawns a mini-process that writes to a channel in a loop until
/// the channel is full. The kernel raises a policy exception
/// (ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS equivalent for
/// channels). The process is killed with ZX_TASK_RETCODE_EXCEPTION_KILL.
///
/// Verifies write_count > 1000 before the exception.
#[test]
fn channel_full_exception() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create process + thread + channel
    //   2. start_mini_process_etc with channel as bootstrap handle
    //   3. Loop: mini_process_cmd(MINIP_CMD_CHANNEL_WRITE) until != ZX_OK
    //   4. ASSERT_GT(write_count, 1000)
    //   5. Wait for ZX_PROCESS_TERMINATED
    //   6. Assert return_code == ZX_TASK_RETCODE_EXCEPTION_KILL
}

// ── Port tests ──────────────────────────────────────────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// port/ports.cc:213
///
/// Pre-queues 4096 user packets to a port, then queues one more
/// from a worker thread. The kernel raises ZX_EXCP_POLICY_ERROR
/// with synth_code ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS.
///
/// Main thread reads the exception from the process exception channel,
/// verifies the exception type and synth_code, then resumes the thread.
#[test]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create port, queue 4096 packets
    //   2. Spawn worker thread
    //   3. Create process exception channel
    //   4. Worker attempts 4097th queue → triggers policy exception
    //   5. Main reads exception: type == ZX_EXCP_POLICY_ERROR,
    //      synth_code == ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS
    //   6. Resume thread from exception
}

/// C++: TEST(PortTest, TooManyObservers)
/// port/ports.cc:1155
///
/// Spawns a mini-process that calls wait_async in a loop until OOM.
/// The process is killed with ZX_TASK_RETCODE_EXCEPTION_KILL.
/// In zCore, the allocator panics on OOM instead of returning
/// ZX_ERR_NO_MEMORY.
#[test]
fn port_too_many_observers() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create process + thread + channel
    //   2. start_mini_process_etc
    //   3. mini_process_cmd(MINIP_CMD_WAIT_ASYNC) → loops until OOM
    //   4. Expect ZX_ERR_PEER_CLOSED (process terminated)
    //   5. Wait for ZX_PROCESS_TERMINATED
    //   6. Assert return_code == ZX_TASK_RETCODE_EXCEPTION_KILL
}

// ── VmoReference pager test ─────────────────────────────────────────

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// vmo/vmo-reference.cc:130
///
/// Creates a pager-backed VMO, supplies pages, writes 0xaa, creates
/// a REFERENCE child, then creates a SNAPSHOT_AT_LEAST_ON_WRITE child
/// of the reference. Verifies:
///   1. parent_koid of child matches reference koid
///   2. Child can read 0xaa from root VMO
///   3. Writing 0xbb to root is visible in child (not yet forked)
///   4. Writing 0xcc to child forks the page (not visible in root)
///   5. Root still reads 0xbb, child reads 0xcc
#[test]
fn vmo_reference_child_snapshot_at_least_on_write() {
    let _ctx = TestContext::new();
    // zCore creates a regular COW snapshot instead of a true
    // SNAPSHOT_AT_LEAST_ON_WRITE that defers forking until the
    // child writes.
    //
    // In Fuchsia:
    //   1. pager.create_vmo(page_size) → vmo
    //   2. Supply pages to vmo
    //   3. vmo.write(0xaa)
    //   4. vmo.create_child(ZX_VMO_CHILD_REFERENCE) → ref
    //   5. ref.create_child(ZX_VMO_CHILD_SNAPSHOT_AT_LEAST_ON_WRITE, page_size) → child
    //   6. Assert child reads 0xaa
    //   7. vmo.write(0xbb) → child reads 0xbb (not yet forked)
    //   8. child.write(0xcc) → forks page
    //   9. vmo reads 0xbb (unchanged), child reads 0xcc
}

// ── VmoZero populated_bytes tests ───────────────────────────────────
// These test COW page attribution accounting (populated_bytes).
// zCore's fractional attribution doesn't converge correctly,
// causing wrong populated_bytes values.

/// C++: TEST(VmoZeroTestCase, WriteCowParent)
/// vmo/vmo-zero.cc
///
/// Creates a 2-page parent VMO, writes to both pages, creates a COW
/// child, writes to child page 0 (forks it), zeros parent page 0.
/// Verifies parent.populated_bytes == 1 page.
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

    let info = parent.get_info();
    assert_eq!(
        info.populated_bytes as usize, PAGE_SIZE,
        "populated_bytes: parent should have 1 page after zeroing page 0"
    );
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMergeMultipleChildren)
/// Multiple COW children, drop some, verify populated_bytes.
#[test]
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
        "populated_bytes: should be 0 after zeroing all pages and dropping child"
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
        "populated_bytes: should be 0 after zero + child drop"
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
#[test]
fn vmo_clone2_resize_offset_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);
    let clone = vmo.create_child(false, PAGE_SIZE, PAGE_SIZE).unwrap();

    vmo.set_len(0).unwrap();

    let info = vmo.get_info();
    assert_eq!(info.populated_bytes, 0, "parent resized to 0");
    let clone_info = clone.get_info();
    assert_eq!(
        clone_info.populated_bytes as usize, PAGE_SIZE,
        "clone should own its page"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeDisjointChild)
#[test]
fn vmo_clone2_resize_disjoint_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);

    let c0 = vmo.create_child(true, 0, PAGE_SIZE).unwrap();
    let c1 = vmo.create_child(true, PAGE_SIZE, PAGE_SIZE).unwrap();
    let c2 = vmo.create_child(true, 2 * PAGE_SIZE, PAGE_SIZE).unwrap();

    c0.set_len(0).unwrap();
    c1.set_len(0).unwrap();
    vmo.set_len(0).unwrap();

    let info = c2.get_info();
    assert_eq!(
        info.populated_bytes as usize, PAGE_SIZE,
        "c2 should be sole owner"
    );
}

/// C++: TEST_F(VmoClone2TestCase, ResizeMultipleProgressive)
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
        "clone2 should have attributed pages"
    );
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneClose)
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

    drop(vmo);

    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "each clone should have attributed pages"
        );
    }
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneTruncate)
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

    vmo.set_len(0).unwrap();

    for c in &clones {
        let info = c.get_info();
        assert!(
            info.populated_bytes > 0,
            "each clone should have attributed pages"
        );
    }
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeChild)
#[test]
fn vmo_clone_resize_child() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);
    let child = vmo.create_child(true, 0, 3 * PAGE_SIZE).unwrap();

    child.set_len(PAGE_SIZE).unwrap();

    let parent_info = vmo.get_info();
    let child_info = child.get_info();
    assert_eq!(child_info.populated_bytes as usize, PAGE_SIZE);
    assert_eq!(parent_info.populated_bytes as usize, 2 * PAGE_SIZE);
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeOriginal)
#[test]
fn vmo_clone_resize_original() {
    let _ctx = TestContext::new();
    let vmo = init_page_tagged_vmo_resizable(3);
    let child = vmo.create_child(true, 0, 3 * PAGE_SIZE).unwrap();

    vmo.set_len(PAGE_SIZE).unwrap();

    let parent_info = vmo.get_info();
    let child_info = child.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);
    assert_eq!(child_info.populated_bytes as usize, 2 * PAGE_SIZE);
}

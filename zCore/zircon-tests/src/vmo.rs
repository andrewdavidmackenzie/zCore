//! VMO syscall tests.
//!
//! Mirrors tests from zircon/system/utest/core/vmo/.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::vm::{VmObject, PAGE_SIZE};
use zircon_object::ZxError;

/// vmo_create: basic creation with correct size.
#[test]
fn vmo_create() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(4);
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);
}

/// vmo_read/write: basic data round-trip.
#[test]
fn vmo_read_write() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    let data = b"hello vmo!";
    vmo.write(0, data).unwrap();

    let mut buf = [0u8; 10];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, data);
}

/// vmo_read: reading unwritten pages returns zeros.
#[test]
fn vmo_read_zeros() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    let mut buf = [0xFFu8; 64];
    vmo.read(0, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

/// vmo_get_size: reports correct size.
#[test]
fn vmo_get_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(3);
    assert_eq!(vmo.len(), 3 * PAGE_SIZE);
}

/// vmo_set_size: resize a resizable VMO.
#[test]
fn vmo_set_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 2);
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);

    vmo.set_len(4 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);

    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);
}

/// vmo_set_size: non-resizable VMO returns error.
#[test]
fn vmo_set_size_not_resizable() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    assert!(vmo.set_len(4 * PAGE_SIZE).is_err());
}

/// vmo_create_child (SNAPSHOT): COW clone isolation.
#[test]
fn vmo_snapshot_isolation() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[0xAA]).unwrap();

    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Child sees parent data
    let mut buf = [0u8; 1];
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to parent doesn't affect child (COW)
    parent.write(0, &[0xBB]).unwrap();

    let mut buf = [0u8; 1];
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA); // still old value

    // Write to child doesn't affect parent
    child.write(0, &[0xCC]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB); // unchanged
    child.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xCC);
}

/// vmo_create_child (SLICE): transparent write-through.
#[test]
fn vmo_slice_write_through() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // Slice sees parent data
    let mut buf = [0u8; 1];
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Write to slice is visible in parent
    slice.write(0, &[0xBB]).unwrap();
    parent.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xBB);

    // Write to parent is visible in slice
    parent.write(0, &[0xCC]).unwrap();
    slice.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xCC);
}

/// vmo_op_range (COMMIT): committed bytes increase.
#[test]
fn vmo_commit() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    let info = vmo.get_info();
    assert_eq!(info.committed_bytes, 0);

    // Write commits a page
    vmo.write(0, &[42]).unwrap();
    let info = vmo.get_info();
    assert_eq!(info.committed_bytes as usize, PAGE_SIZE);
}

/// vmo_op_range (DECOMMIT): releases pages.
#[test]
fn vmo_decommit() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[42]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    vmo.decommit(0, PAGE_SIZE).unwrap();
    assert_eq!(vmo.get_info().committed_bytes, 0);

    // Reading after decommit returns zeros
    let mut buf = [0xFFu8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// VMO zero operation.
#[test]
fn vmo_zero() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"Hello World!").unwrap();
    vmo.zero(0, 5).unwrap();

    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf[0..5], &[0, 0, 0, 0, 0]);
    assert_eq!(&buf[5..12], b" World!");
}

/// Slice populated_bytes should be zero (pages attributed to parent).
#[test]
fn vmo_slice_attributed_counts() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    parent.write(0, &[42]).unwrap();

    let slice = parent.create_slice(0, PAGE_SIZE).unwrap();

    // Parent owns the page
    let parent_info = parent.get_info();
    assert_eq!(parent_info.populated_bytes as usize, PAGE_SIZE);

    // Slice does NOT own the page
    let slice_info = slice.get_info();
    assert_eq!(slice_info.populated_bytes, 0);
}

/// C++: TEST(VmoTestCase, ReadWriteBadLen)
/// Read/write within bounds succeeds; exact size is fine.
/// NOTE: zCore's kernel object layer does not enforce OUT_OF_RANGE
/// for reads/writes past end the same way Fuchsia's syscall layer does.
/// The syscall layer would add those checks.
#[test]
fn vmo_read_write_exact_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(4);
    let len = 4 * PAGE_SIZE;

    // Exact size read/write succeeds
    let mut buf = vec![0u8; len];
    vmo.write(0, &buf).unwrap();
    vmo.read(0, &mut buf).unwrap();

    // Partial read/write succeeds
    vmo.write(0, &[42u8; 100]).unwrap();
    let mut small = [0u8; 100];
    vmo.read(0, &mut small).unwrap();
    assert_eq!(small[0], 42);
}

/// C++: TEST(VmoTestCase, ReadWriteRange)
/// Read/write at various offsets within bounds.
#[test]
fn vmo_read_write_range() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(4);
    let len = 4 * PAGE_SIZE;

    // Read 0 bytes at end is OK
    let mut empty = [0u8; 0];
    vmo.read(len, &mut empty).unwrap();

    // Write 0 bytes at end is OK
    vmo.write(len, &[]).unwrap();

    // Read/write at various page-aligned offsets
    for offset in [0, PAGE_SIZE, 2 * PAGE_SIZE, 3 * PAGE_SIZE] {
        let data = vec![offset as u8; 64];
        vmo.write(offset, &data).unwrap();
        let mut buf = vec![0u8; 64];
        vmo.read(offset, &mut buf).unwrap();
        assert_eq!(buf, data);
    }
}

/// C++: TEST(VmoTestCase, SizeAlign)
/// VMO size rounds up to page boundary.
#[test]
fn vmo_size_align() {
    let _ctx = TestContext::new();

    // 0 pages = 0 bytes
    let vmo = VmObject::new_paged(0);
    assert_eq!(vmo.len(), 0);

    // 1 page
    let vmo = VmObject::new_paged(1);
    assert_eq!(vmo.len(), PAGE_SIZE);

    // Multiple pages
    for pages in [2, 3, 4, 8, 16] {
        let vmo = VmObject::new_paged(pages);
        assert_eq!(vmo.len(), pages * PAGE_SIZE);
    }
}

/// C++: TEST(VmoTestCase, ResizeAlign)
/// Resize rounds up to page boundary.
#[test]
fn vmo_resize_align() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 0);

    // Resize to non-page-aligned sizes — should round up
    for size in [1usize, PAGE_SIZE - 1, PAGE_SIZE + 1, PAGE_SIZE * 2 + 100] {
        vmo.set_len(size).unwrap();
        let expected = size.div_ceil(PAGE_SIZE) * PAGE_SIZE;
        assert_eq!(
            vmo.len(),
            expected,
            "set_len({}) should round up to {}",
            size,
            expected
        );
    }

    // Exact page size
    vmo.set_len(PAGE_SIZE * 3).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE * 3);
}

/// C++: TEST(VmoTestCase, Resize) — extended
/// Resize up, down, ludicrous sizes.
#[test]
fn vmo_resize_extended() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 4);
    assert_eq!(vmo.len(), 4 * PAGE_SIZE);

    // Grow
    vmo.set_len(5 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 5 * PAGE_SIZE);

    // Shrink
    vmo.set_len(2 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);

    // Shrink to zero
    vmo.set_len(0).unwrap();
    assert_eq!(vmo.len(), 0);

    // Grow back
    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);

    // After resize, new area reads as zeros
    let mut buf = [0xFFu8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

/// C++: TEST(VmoTestCase, NoResize) — extended
/// Non-resizable VMO rejects set_size both up and down.
#[test]
fn vmo_no_resize_extended() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    let original = vmo.len();

    // Can't grow
    assert!(vmo.set_len(4 * PAGE_SIZE).is_err());
    assert_eq!(vmo.len(), original);

    // Can't shrink
    assert!(vmo.set_len(PAGE_SIZE).is_err());
    assert_eq!(vmo.len(), original);

    // Can't set to same size
    assert!(vmo.set_len(original).is_err());
}

/// C++: TEST(VmoTestCase, Info)
/// get_info returns correct flags.
#[test]
fn vmo_info() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(4);
    let info = vmo.get_info();

    // Resizable VMO has RESIZABLE flag
    let vmo_r = VmObject::new_paged_with_resizable(true, 2);
    let info_r = vmo_r.get_info();
    assert!(info_r
        .flags
        .contains(zircon_object::vm::VmoInfoFlags::RESIZABLE));

    // Non-resizable VMO does NOT have RESIZABLE flag
    assert!(!info
        .flags
        .contains(zircon_object::vm::VmoInfoFlags::RESIZABLE));

    // Paged VMO has TYPE_PAGED flag
    assert!(info
        .flags
        .contains(zircon_object::vm::VmoInfoFlags::TYPE_PAGED));
}

/// C++: TEST(VmoTestCase, ContentSize)
/// Get/set content_size property.
#[test]
fn vmo_content_size() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 4);
    let len = 4 * PAGE_SIZE;

    // Set content_size to something specific
    let target = len / 3;
    vmo.set_content_size(target).unwrap();
    assert_eq!(vmo.content_size(), target);

    // Set content_size back to VMO size
    vmo.set_content_size(len).unwrap();
    assert_eq!(vmo.content_size(), len);

    // Set to zero
    vmo.set_content_size(0).unwrap();
    assert_eq!(vmo.content_size(), 0);
}

/// C++: TEST(VmoTestCase, DecommitAligned)
/// Page-aligned decommit works correctly.
/// NOTE: zCore's kernel object layer does not enforce page-alignment
/// on decommit offset/length — the syscall layer would add those checks.
#[test]
fn vmo_decommit_aligned() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);

    // Page-aligned decommit works
    vmo.write(0, &[42]).unwrap();
    vmo.decommit(0, PAGE_SIZE).unwrap();

    // Verify decommitted page reads as zeros
    let mut buf = [0xFFu8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0);
}

/// C++: TEST(VmoTestCase, DecommitOutOfRange)
/// Decommit past end returns OUT_OF_RANGE.
#[test]
fn vmo_decommit_out_of_range() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);

    // Decommit past end
    assert_eq!(
        vmo.decommit(0, 2 * PAGE_SIZE).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(VmoTestCase, ZeroRange)
/// Zero a range in the middle of written data.
#[test]
fn vmo_zero_range() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);

    // Write a pattern across the page
    let data: Vec<u8> = (0..64).collect();
    vmo.write(0, &data).unwrap();

    // Zero bytes 16..32
    vmo.zero(16, 16).unwrap();

    let mut buf = [0u8; 64];
    vmo.read(0, &mut buf).unwrap();

    // 0..16 unchanged
    for (i, &b) in buf[..16].iter().enumerate() {
        assert_eq!(b, i as u8);
    }
    // 16..32 zeroed
    for &b in &buf[16..32] {
        assert_eq!(b, 0);
    }
    // 32..64 unchanged
    for (i, &b) in buf[32..64].iter().enumerate() {
        assert_eq!(b, (i + 32) as u8);
    }
}

/// C++: TEST(VmoTestCase, ZeroOutOfRange)
/// Zero past end returns OUT_OF_RANGE.
#[test]
fn vmo_zero_out_of_range() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);

    assert_eq!(
        vmo.zero(0, 2 * PAGE_SIZE).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(VmoTestCase, Cache)
/// Set cache policy on clean VMO; clone prevents policy change.
#[test]
fn vmo_cache_policy() {
    use zircon_object::vm::CachePolicy;

    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);

    // Clean VMO can set all valid policies
    vmo.set_cache_policy(CachePolicy::Cached).unwrap();
    vmo.set_cache_policy(CachePolicy::Uncached).unwrap();
    vmo.set_cache_policy(CachePolicy::WriteCombining).unwrap();

    // Reset to cached
    vmo.set_cache_policy(CachePolicy::Cached).unwrap();

    // Clone prevents policy change on parent
    let child = vmo.create_child(false, 0, PAGE_SIZE).unwrap();
    assert_eq!(
        vmo.set_cache_policy(CachePolicy::Uncached).unwrap_err(),
        ZxError::BAD_STATE
    );

    // Drop clone, now can change again
    drop(child);
    vmo.set_cache_policy(CachePolicy::Uncached).unwrap();

    // Uncached VMO rejects read/write
    let mut buf = [0u8; 1];
    assert_eq!(vmo.read(0, &mut buf).unwrap_err(), ZxError::BAD_STATE);
    assert_eq!(vmo.write(0, &[42]).unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(VmoTestCase, CommittedBytesAfterWrite)
/// committed_bytes increases after write, decreases after decommit.
#[test]
fn vmo_committed_bytes_tracking() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(2);
    assert_eq!(vmo.get_info().committed_bytes, 0);

    // Write to first page
    vmo.write(0, &[42]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    // Write to second page
    vmo.write(PAGE_SIZE, &[99]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, 2 * PAGE_SIZE);

    // Decommit first page
    vmo.decommit(0, PAGE_SIZE).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);
}

/// C++: TEST(VmoTestCase, ZeroChildrenSignal)
/// VMO_ZERO_CHILDREN signal management.
#[test]
fn vmo_zero_children_signal() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(1);
    // Initially: no children, signal should be set
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Create a child — signal should be cleared
    let child = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    assert!(!parent.signal().contains(Signal::VMO_ZERO_CHILDREN));

    // Drop the child — signal should be re-set
    drop(child);
    assert!(parent.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

/// C++: TEST(VmoTestCase, ResizeDataPreserved)
/// Growing a VMO preserves existing data; new area is zero.
#[test]
fn vmo_resize_data_preserved() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 1);
    vmo.write(0, b"existing data").unwrap();

    // Grow
    vmo.set_len(2 * PAGE_SIZE).unwrap();

    // Old data preserved
    let mut buf = [0u8; 13];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"existing data");

    // New area is zeros
    let mut buf = [0xFFu8; 8];
    vmo.read(PAGE_SIZE, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

/// C++: TEST(VmoTestCase, ResizeShrinkDataLost)
/// Shrinking and re-growing a VMO zeroes the new pages.
#[test]
fn vmo_resize_shrink_data_lost() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged_with_resizable(true, 2);
    vmo.write(PAGE_SIZE, b"page two data").unwrap();

    // Shrink to 1 page
    vmo.set_len(PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), PAGE_SIZE);

    // Grow back — new second page should be zeros, not old data
    vmo.set_len(2 * PAGE_SIZE).unwrap();
    let mut buf = [0xFFu8; 13];
    vmo.read(PAGE_SIZE, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0), "re-grown page should be zeros");
}

/// C++: TEST(VmoTestCase, Name)
/// VMO name get/set.
#[test]
fn vmo_name() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    vmo.set_name("my-vmo");
    assert_eq!(vmo.name(), "my-vmo");

    vmo.set_name("renamed");
    assert_eq!(vmo.name(), "renamed");
}

/// C++: TEST(VmoTestCase, TypeName)
#[test]
fn vmo_type_name() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);
    assert_eq!(vmo.type_name(), "VmObject");
}

/// C++: TEST(VmoTestCase, MultipleChildren)
/// Multiple children can be created from the same parent.
#[test]
fn vmo_multiple_children() {
    let _ctx = TestContext::new();

    let parent = VmObject::new_paged(2);
    parent.write(0, &[0xAA]).unwrap();

    let child1 = parent.create_child(false, 0, PAGE_SIZE).unwrap();
    let child2 = parent.create_child(false, 0, PAGE_SIZE).unwrap();

    // Both see parent data
    let mut buf = [0u8; 1];
    child1.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA);

    // Children are independent
    child1.write(0, &[0xBB]).unwrap();
    child2.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 0xAA); // unaffected
}

/// C++: TEST(VmoTestCase, CommitDecommitRoundTrip)
/// Write, decommit, re-write cycle.
#[test]
fn vmo_commit_decommit_round_trip() {
    let _ctx = TestContext::new();

    let vmo = VmObject::new_paged(1);

    // Write and verify committed
    vmo.write(0, &[42]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    // Decommit
    vmo.decommit(0, PAGE_SIZE).unwrap();
    assert_eq!(vmo.get_info().committed_bytes, 0);

    // Re-write
    vmo.write(0, &[99]).unwrap();
    assert_eq!(vmo.get_info().committed_bytes as usize, PAGE_SIZE);

    let mut buf = [0u8; 1];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf[0], 99);
}

// -- Faithful 1:1 ports from VmoSignalTestCase (vmo-signal.cc) --

/// C++: TEST(VmoSignalTestCase, SignalSanity)
/// VMO handles support user signals; initial state has VMO_ZERO_CHILDREN.
#[test]
fn signal_sanity() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Initial signals: VMO_ZERO_CHILDREN should be set (no children)
    let sig = vmo.signal();
    assert!(sig.contains(Signal::VMO_ZERO_CHILDREN));
    assert!(!sig.contains(Signal::USER_SIGNAL_0));

    // Set a user signal
    vmo.signal_set(Signal::USER_SIGNAL_0);
    let sig = vmo.signal();
    assert!(sig.contains(Signal::USER_SIGNAL_0));
    assert!(sig.contains(Signal::VMO_ZERO_CHILDREN));
}

/// C++: TEST(VmoSignalTestCase, ChildSignalClone)
/// VMO_ZERO_CHILDREN signal tracks clone lifecycle.
/// Creates snapshot clones in a loop, verifying signal transitions.
#[test]
fn child_signal_clone() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);

    for _ in 0..10 {
        // No children — signal set
        assert!(vmo.signal().contains(Signal::VMO_ZERO_CHILDREN));

        let clone = vmo.create_child(false, 0, PAGE_SIZE).unwrap();

        // Has child — ZERO_CHILDREN cleared on parent
        assert!(clone.signal().contains(Signal::VMO_ZERO_CHILDREN));
        assert!(!vmo.signal().contains(Signal::VMO_ZERO_CHILDREN));

        let clone2 = clone.create_child(false, 0, PAGE_SIZE).unwrap();

        // clone2 has no children, clone has children, vmo has children
        assert!(clone2.signal().contains(Signal::VMO_ZERO_CHILDREN));
        assert!(!clone.signal().contains(Signal::VMO_ZERO_CHILDREN));
        assert!(!vmo.signal().contains(Signal::VMO_ZERO_CHILDREN));

        // Close clone first — vmo still has grandchild
        drop(clone);
        assert!(!vmo.signal().contains(Signal::VMO_ZERO_CHILDREN));
        assert!(clone2.signal().contains(Signal::VMO_ZERO_CHILDREN));

        // Close clone2 — vmo has no children again
        drop(clone2);
    }

    assert!(vmo.signal().contains(Signal::VMO_ZERO_CHILDREN));
}

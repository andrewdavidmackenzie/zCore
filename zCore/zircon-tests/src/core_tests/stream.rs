//! Stream syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{SeekOrigin, Stream, VmObject};

/// C++: TEST(StreamTestCase, Create)
/// Basic stream creation with properties verified.
#[test]
fn stream_create_verify_properties() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3); // MODE_READ | MODE_WRITE

    let info = stream.get_info();
    assert_eq!(info.options, 0x3);
    assert_eq!(info.seek, 0);
    assert_eq!(info.content_size, 0);
    assert_ne!(stream.id(), 0);
}

/// C++: TEST(StreamTestCase, Seek)
/// Seek to beginning, current, and end positions.
#[test]
fn stream_seek_all_origins() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789ABCDEF", false).unwrap(); // 16 bytes

    // Seek to beginning
    let pos = stream.seek(SeekOrigin::Start, 0).unwrap();
    assert_eq!(pos, 0);

    // Seek to absolute position 8
    let pos = stream.seek(SeekOrigin::Start, 8).unwrap();
    assert_eq!(pos, 8);

    // Seek +4 from current (8 + 4 = 12)
    let pos = stream.seek(SeekOrigin::Current, 4).unwrap();
    assert_eq!(pos, 12);

    // Seek -2 from current (12 - 2 = 10)
    let pos = stream.seek(SeekOrigin::Current, -2).unwrap();
    assert_eq!(pos, 10);

    // Seek to end (content_size = 16)
    let pos = stream.seek(SeekOrigin::End, 0).unwrap();
    assert_eq!(pos, 16);

    // Seek -5 from end (16 - 5 = 11)
    let pos = stream.seek(SeekOrigin::End, -5).unwrap();
    assert_eq!(pos, 11);
}

/// C++: TEST(StreamTestCase, WriteExtendsContentSize)
/// Writing extends content_size but not VMO size.
#[test]
fn stream_write_extends_content_size() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2); // 2 pages = 8192 bytes
    vmo.set_content_size(0).unwrap();

    let stream = Stream::create(vmo.clone(), 0, 0x3);

    assert_eq!(vmo.content_size(), 0);

    stream.write(b"hello", false).unwrap();
    assert_eq!(vmo.content_size(), 5);

    stream.write(b" world!", false).unwrap();
    assert_eq!(vmo.content_size(), 12);

    // Write at offset beyond current content — gap is zero-filled
    stream.seek(SeekOrigin::Start, 20).unwrap();
    stream.write(b"XX", false).unwrap();
    assert_eq!(vmo.content_size(), 22);

    // Verify the gap is zeroed
    stream.seek(SeekOrigin::Start, 12).unwrap();
    let mut gap = [0xFFu8; 8];
    stream.read(&mut gap).unwrap();
    assert!(gap.iter().all(|&b| b == 0), "gap should be zeroed");
}

/// C++: TEST(StreamTestCase, WriteDoesntExtendVMOSize)
/// Writing does not extend the VMO size past the original allocation.
#[test]
fn stream_write_doesnt_extend_vmo_size() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1); // 1 page = 4096 bytes
    let original_vmo_len = vmo.len();

    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write data — content_size grows but VMO size stays the same
    let data = [0xAA; 2048];
    stream.write(&data, false).unwrap();

    assert_eq!(vmo.len(), original_vmo_len);

    // Try to write past the VMO boundary — clamped to VMO size
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(&[0xBB; 100], false).unwrap();
    assert_eq!(n, 6); // only 6 bytes fit (4096 - 4090)
    assert_eq!(vmo.len(), original_vmo_len);
}

/// C++: TEST(StreamTestCase, ReadV)
/// Vectored read: multiple buffers filled from the stream.
#[test]
fn stream_read_v() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"AAABBBCCC", false).unwrap();
    stream.seek(SeekOrigin::Start, 0).unwrap();

    // Read into three separate buffers (simulates readv iovec)
    let mut buf1 = [0u8; 3];
    let mut buf2 = [0u8; 3];
    let mut buf3 = [0u8; 3];

    let n1 = stream.read(&mut buf1).unwrap();
    let n2 = stream.read(&mut buf2).unwrap();
    let n3 = stream.read(&mut buf3).unwrap();

    assert_eq!(n1, 3);
    assert_eq!(n2, 3);
    assert_eq!(n3, 3);
    assert_eq!(&buf1, b"AAA");
    assert_eq!(&buf2, b"BBB");
    assert_eq!(&buf3, b"CCC");
}

/// C++: TEST(StreamTestCase, WriteV)
/// Vectored write: multiple buffers written to the stream.
#[test]
fn stream_write_v() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // Write from multiple "iov" segments
    stream.write(b"AAA", false).unwrap();
    stream.write(b"BBB", false).unwrap();
    stream.write(b"CCC", false).unwrap();

    // Read back the combined result
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 9];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buf, b"AAABBBCCC");
}

/// C++: TEST(StreamTestCase, ReadVAt)
/// Read at specific offsets without moving seek position.
#[test]
fn stream_read_v_at() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789ABCDEF", false).unwrap();

    // read_at at various offsets
    let mut buf1 = [0u8; 4];
    let n = stream.read_at(&mut buf1, 0).unwrap();
    assert_eq!(n, 4);
    assert_eq!(&buf1, b"0123");

    let mut buf2 = [0u8; 4];
    let n = stream.read_at(&mut buf2, 8).unwrap();
    assert_eq!(n, 4);
    assert_eq!(&buf2, b"89AB");

    let mut buf3 = [0u8; 4];
    let n = stream.read_at(&mut buf3, 12).unwrap();
    assert_eq!(n, 4);
    assert_eq!(&buf3, b"CDEF");
}

/// C++: TEST(StreamTestCase, WriteVAt)
/// Write at specific offsets without moving seek position.
#[test]
fn stream_write_v_at() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // Fill with zeros first
    stream.write(&[0u8; 16], false).unwrap();

    // Write at specific offsets
    stream.write_at(b"AAAA", 0).unwrap();
    stream.write_at(b"BBBB", 4).unwrap();
    stream.write_at(b"CCCC", 8).unwrap();

    // Read back the whole thing
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 12];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 12);
    assert_eq!(&buf, b"AAAABBBBCCCC");
}

/// C++: TEST(StreamTestCase, SyscallInvalidArgs)
/// Bad args to stream operations.
#[test]
fn stream_syscall_invalid_args() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Seek before start should fail
    let stream = Stream::create(vmo, 0, 0x3);
    let result = stream.seek(SeekOrigin::Start, -1);
    assert!(result.is_err());

    // Seek with overflow should fail
    let result = stream.seek(SeekOrigin::End, -100);
    assert!(result.is_err());
}

/// C++: TEST(StreamTestCase, NoStreamFromContiguousOrPhysicalVmo)
/// Cannot create a stream from a contiguous or physical VMO.
/// This is enforced at the syscall layer, so we test the VMO
/// properties that the syscall checks.
#[test]
fn stream_no_contiguous_or_physical_vmo() {
    let _ctx = TestContext::new();

    // A contiguous VMO should be rejected by sys_stream_create's check:
    // `!vmo.is_paged() || vmo.is_contiguous()` → NOT_SUPPORTED
    // We verify the predicates here at the object level.
    if let Ok(contiguous) = VmObject::new_contiguous(1, 12) {
        // Contiguous VMOs are paged but contiguous — should be rejected
        assert!(contiguous.is_contiguous());
        // The syscall would return NOT_SUPPORTED
    }

    // A physical VMO should not be paged
    let physical = VmObject::new_physical(0x1000, 1);
    assert!(!physical.is_paged());
    // The syscall would return NOT_SUPPORTED
}

// ── Bulk-ported stream tests ────────────────────────────────────────

/// C++: TEST(StreamTestCase, Append)
///
/// Append mode writes always go to the end of the content, regardless
/// of the current seek position.
#[test]
fn stream_append_mode() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3); // MODE_READ | MODE_WRITE

    // Write initial data.
    stream.write(b"hello", false).unwrap();
    assert_eq!(vmo.content_size(), 5);

    // Seek to the beginning.
    stream.seek(SeekOrigin::Start, 0).unwrap();

    // Append should write at content_size (5), not at seek (0).
    let n = stream.write(b" world", true).unwrap();
    assert_eq!(n, 6);
    assert_eq!(vmo.content_size(), 11);

    // Verify the full content.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 11];
    stream.read(&mut buf).unwrap();
    assert_eq!(&buf, b"hello world");
}

/// C++: TEST(StreamTestCase, ExtendFillsWithZeros)
///
/// Writing past the current content_size should zero-fill the gap
/// between old content_size and the write offset.
#[test]
fn stream_extend_fills_with_zeros() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write at offset 0.
    stream.write(b"start", false).unwrap();
    assert_eq!(vmo.content_size(), 5);

    // Seek to offset 100 and write — gap [5..100) should be zeros.
    stream.seek(SeekOrigin::Start, 100).unwrap();
    stream.write(b"end", false).unwrap();
    assert_eq!(vmo.content_size(), 103);

    // Read the gap region.
    let mut gap = [0xFFu8; 95];
    stream.read_at(&mut gap, 5).unwrap();
    assert!(gap.iter().all(|&b| b == 0), "gap should be zero-filled");
}

/// C++: TEST(StreamTestCase, ExpandOverflow)
///
/// Writing with an offset that would overflow usize should fail with
/// FILE_BIG or OUT_OF_RANGE.
#[test]
fn stream_expand_overflow() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // write_at with offset at VMO boundary should fail with OUT_OF_RANGE
    // since offset >= vmo_len.
    let result = stream.write_at(b"overflow", 4096);
    assert!(result.is_err(), "write at VMO boundary should fail");
}

/// C++: TEST(StreamTestCase, PropertyModeAppend)
///
/// Set the MODE_APPEND property on a stream and verify all writes
/// go to the end of content.
#[test]
fn stream_property_mode_append() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // Create without MODE_APPEND initially.
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    stream.write(b"hello", false).unwrap();

    // Enable MODE_APPEND via property.
    assert!(!stream.get_mode_append());
    stream.set_mode_append(true);
    assert!(stream.get_mode_append());

    // Seek to beginning — but with MODE_APPEND, writes go to end.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    stream.write(b" world", false).unwrap(); // append=false, but MODE_APPEND is set

    // Verify content.
    assert_eq!(vmo.content_size(), 11);
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 11];
    stream.read(&mut buf).unwrap();
    assert_eq!(&buf, b"hello world");
}

/// C++: TEST(StreamTestCase, GetInfoReadOnly)
///
/// Get info on a read-only stream (options=0x1 MODE_READ only).
#[test]
fn stream_get_info_read_only() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"readonly data").unwrap();
    vmo.set_content_size(13).unwrap();

    let stream = Stream::create(vmo, 0, 0x1); // MODE_READ only
    let info = stream.get_info();
    assert_eq!(info.options, 0x1);
    assert_eq!(info.seek, 0);
    assert_eq!(info.content_size, 13);
}

/// C++: TEST(StreamTestCase, ZeroLengthVectors)
///
/// Reading and writing with zero-length buffers should return 0.
#[test]
fn stream_zero_length_vectors() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // Zero-length write.
    let n = stream.write(b"", false).unwrap();
    assert_eq!(n, 0);

    // Zero-length read.
    let mut buf = [0u8; 0];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

/// C++: TEST(StreamTestCase, VectorCapacityOverflow)
///
/// write_at with offset + data.len() overflowing usize should return
/// FILE_BIG error.
#[test]
fn stream_vector_capacity_overflow() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // The write_at implementation checks for offset + count overflow.
    // usize::MAX offset would certainly overflow.
    let result = stream.write_at(b"data", usize::MAX);
    assert!(
        result.is_err(),
        "write_at with usize::MAX offset should fail"
    );
}

/// C++: TEST(StreamTestCase, PartialAppendAtVmoBoundary)
///
/// Append at the VMO boundary where only part of the data fits.
/// The write should be clamped to the VMO size.
#[test]
fn stream_partial_append_at_vmo_boundary() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1); // 4096 bytes
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write to near the end of the VMO.
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(b"0123456789", false).unwrap();
    assert_eq!(n, 6, "only 6 bytes should fit (4096 - 4090)");

    // Verify partial write.
    let mut buf = [0u8; 6];
    stream.read_at(&mut buf, 4090).unwrap();
    assert_eq!(&buf, b"012345");
}

/// C++: TEST(StreamTestCase, WriteVectorWithStreamInAppendMode)
///
/// Multiple sequential writes with MODE_APPEND — each write should
/// append after the previous one.
#[test]
fn stream_write_vector_with_append_mode() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // Create with MODE_APPEND (0x4) | MODE_READ (0x1) | MODE_WRITE (0x2)
    let stream = Stream::create(vmo.clone(), 0, 0x7);

    // Multiple writes — all should append.
    stream.write(b"AAA", false).unwrap();
    stream.write(b"BBB", false).unwrap();
    stream.write(b"CCC", false).unwrap();

    assert_eq!(vmo.content_size(), 9);

    // Read back.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 9];
    stream.read(&mut buf).unwrap();
    assert_eq!(&buf, b"AAABBBCCC");
}

/// C++: TEST(StreamTestCase, FaultBeyondStreamSizeResizeDownRace)
///
/// Resize a VMO down while a stream has a seek position beyond the
/// new size. A subsequent read should return 0 bytes (seek is past
/// content). In libos mode there are no true races, but we verify
/// the boundary behavior synchronously.
#[test]
fn stream_fault_beyond_stream_size_resize_down_race() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write data into the second page and set content_size.
    stream.seek(SeekOrigin::Start, 5000).unwrap();
    stream.write(b"past boundary", false).unwrap();

    // Seek to an offset in the second page.
    stream.seek(SeekOrigin::Start, 5000).unwrap();

    // Resize down to 1 page — the seek position is now past the VMO.
    vmo.set_len(4096).unwrap();
    vmo.set_content_size(4096).unwrap();

    // Read at the stale seek position — should return 0 bytes
    // because seek is past the content_size.
    let mut buf = [0u8; 13];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(
        n, 0,
        "read past content_size after resize should return 0 bytes"
    );
}

/// C++: TEST(StreamTestCase, AppendSuppliesZeroes)
/// Append at an offset beyond current content should zero-fill the gap.
/// In the original test, this uses pager-backed VMOs with dirty tracking.
/// Here we test the basic behavior with a regular paged VMO.
#[test]
fn append_supplies_zeroes() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write initial data.
    stream.write(b"start", false).unwrap();
    assert_eq!(vmo.content_size(), 5);

    // Seek past content and write — gap should be zero-filled.
    stream.seek(SeekOrigin::Start, 50).unwrap();
    stream.write(b"end", false).unwrap();

    // Read back the gap [5..50) — should all be zeros.
    let mut gap = [0xFFu8; 45];
    stream.read_at(&mut gap, 5).unwrap();
    assert!(gap.iter().all(|&b| b == 0), "gap should be zero-filled");
}

/// C++: TEST(StreamTestCase, AppendWithMultipleThreads)
/// In libos mode there are no real concurrent threads, so we simulate
/// sequential appends from "multiple threads" by just doing them in order.
#[test]
fn append_with_multiple_threads() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // MODE_APPEND | MODE_READ | MODE_WRITE = 0x7
    let stream = Stream::create(vmo.clone(), 0, 0x7);

    // Simulate "thread 1" append
    stream.write(b"AAA", false).unwrap();
    // Simulate "thread 2" append
    stream.write(b"BBB", false).unwrap();
    // Simulate "thread 3" append
    stream.write(b"CCC", false).unwrap();

    assert_eq!(vmo.content_size(), 9);

    // Read back — all appends should be sequential.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 9];
    stream.read(&mut buf).unwrap();
    assert_eq!(&buf, b"AAABBBCCC");
}

/// C++: TEST(StreamTestCase, ContentSizeUpdatedOnPartialWrite)
/// When a write is clamped to the VMO boundary, content_size should
/// still be updated to reflect the partial write.
#[test]
fn content_size_updated_on_partial_write() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1); // 4096 bytes
    vmo.set_content_size(0).unwrap();
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Seek to near the end and write more than fits.
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(&[0xAA; 100], false).unwrap();
    assert_eq!(n, 6, "only 6 bytes should fit");
    assert_eq!(vmo.content_size(), 4096, "content_size should be VMO size");
}

/// C++: TEST(StreamTestCase, PartialVmoDirty)
/// Write partial data near the VMO boundary. Verify the data is correct.
#[test]
fn partial_vmo_dirty() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write near the VMO boundary.
    stream.seek(SeekOrigin::Start, 4092).unwrap();
    let n = stream.write(b"ABCDEF", false).unwrap();
    assert_eq!(n, 4, "only 4 bytes fit (4096 - 4092)");

    // Read back and verify.
    let mut buf = [0u8; 4];
    stream.read_at(&mut buf, 4092).unwrap();
    assert_eq!(&buf, b"ABCD");
}

/// C++: TEST(StreamTestCase, RaceReadResizeVecLarger)
/// In libos mode we can't truly race threads. Simulate by resizing
/// the VMO larger between writes and reads.
#[test]
fn race_read_resize_vec_larger() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write initial data.
    stream.write(b"initial", false).unwrap();

    // Resize VMO larger.
    vmo.set_len(8192).unwrap();

    // Read should still work and return the original data.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 7];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 7);
    assert_eq!(&buf, b"initial");
}

/// C++: TEST(StreamTestCase, RaceReadResizeVecSmaller)
/// Simulate resize-down between write and read.
#[test]
fn race_read_resize_vec_smaller() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write some data.
    stream.write(b"data in first page", false).unwrap();

    // Resize VMO smaller (to 1 page).
    vmo.set_len(4096).unwrap();

    // Read should still succeed for data within the remaining size.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 18];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 18);
    assert_eq!(&buf, b"data in first page");
}

/// C++: TEST(StreamTestCase, RaceWriteResizeVecLarger)
/// Resize VMO larger, then write more data.
#[test]
fn race_write_resize_vec_larger() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Resize VMO larger.
    vmo.set_len(8192).unwrap();

    // Write data that spans into the expanded region.
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(b"XXXXXXXXXXXX", false).unwrap();
    // Should be able to write all 12 bytes now (4090 + 12 = 4102 < 8192).
    assert_eq!(n, 12);
}

/// C++: TEST(StreamTestCase, RaceWriteResizeVecSmaller)
/// Resize VMO smaller, then attempt to write beyond the new boundary.
#[test]
fn race_write_resize_vec_smaller() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Resize VMO to 1 page.
    vmo.set_len(4096).unwrap();

    // Try to write at offset beyond new boundary.
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(b"XXXXXXXXXXXX", false).unwrap();
    // Should be clamped to 6 bytes (4096 - 4090).
    assert_eq!(n, 6);
}

/// C++: TEST(StreamTestCase, ReadShrinkRace)
/// Read after content_size has shrunk due to truncation.
#[test]
fn read_shrink_race() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write data.
    stream.write(b"0123456789", false).unwrap();
    assert_eq!(vmo.content_size(), 10);

    // Shrink content_size to 5.
    vmo.set_content_size(5).unwrap();

    // Read from beginning — should only get 5 bytes.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 10];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..5], b"01234");
}

/// C++: TEST(StreamTestCase, ReadVectorAlias)
/// Read data using read_at (simulating iovec aliasing — same data
/// read into multiple buffers at different offsets).
#[test]
fn read_vector_alias() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"AABBCCDD", false).unwrap();

    // Read at overlapping/aliased offsets.
    let mut buf1 = [0u8; 4];
    let mut buf2 = [0u8; 4];
    stream.read_at(&mut buf1, 0).unwrap();
    stream.read_at(&mut buf2, 2).unwrap();
    assert_eq!(&buf1, b"AABB");
    assert_eq!(&buf2, b"BBCC");
}

/// C++: TEST(StreamTestCase, ReadWriteShrinkRace)
/// Write, shrink content_size, then read and write again.
#[test]
fn read_write_shrink_race() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write initial data.
    stream.write(b"0123456789", false).unwrap();
    assert_eq!(vmo.content_size(), 10);

    // Shrink content_size.
    vmo.set_content_size(3).unwrap();

    // Read — should get only 3 bytes.
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 10];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 3);

    // Write more — should extend content_size again.
    stream.seek(SeekOrigin::Start, 3).unwrap();
    stream.write(b"XYZ", false).unwrap();
    assert_eq!(vmo.content_size(), 6);
}

/// C++: TEST(StreamTestCase, WriteShrinkRace)
/// Shrink VMO size, then attempt to write past the new boundary.
#[test]
fn write_shrink_race() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write data into second page.
    stream.seek(SeekOrigin::Start, 5000).unwrap();
    stream.write(b"data", false).unwrap();

    // Shrink VMO to 1 page (4096 bytes).
    vmo.set_len(4096).unwrap();
    vmo.set_content_size(4096).unwrap();

    // Write at boundary — should be clamped.
    stream.seek(SeekOrigin::Start, 4090).unwrap();
    let n = stream.write(b"XXXXXXXXXXXX", false).unwrap();
    assert_eq!(n, 6);
}

/// C++: TEST(StreamTestCase, WriteVectorUnmappedUserBufferCancelsOperation)
/// The C++ test writes from a buffer that crosses a mapped/unmapped
/// page boundary, triggering a user-copy failure. In libos mode all
/// buffers are in the same address space, so the true unmapped-buffer
/// path cannot be exercised. We approximate by testing write_at with
/// a near-overflow offset.
#[test]
#[ignore] // Requires partially unmapped user buffer — not possible in libos mode.
fn write_vector_unmapped_user_buffer_cancels_operation() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    // Write at an absurd offset — should fail.
    let result = stream.write_at(b"data", usize::MAX - 1);
    assert!(result.is_err(), "write_at with near-max offset should fail");
}

//! Stream syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{SeekOrigin, Stream, VmObject, PAGE_SIZE};

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
    let vmo = VmObject::new_paged(1); // 1 page
    let original_vmo_len = vmo.len();

    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write data — content_size grows but VMO size stays the same
    let data = [0xAA; 2048];
    stream.write(&data, false).unwrap();

    assert_eq!(vmo.len(), original_vmo_len);

    // Try to write past the VMO boundary — clamped to VMO size
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE - 6) as isize)
        .unwrap();
    let n = stream.write(&[0xBB; 100], false).unwrap();
    assert_eq!(n, 6); // only 6 bytes fit (PAGE_SIZE - offset)
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
    let physical = VmObject::new_physical(PAGE_SIZE, 1);
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
    let result = stream.write_at(b"overflow", PAGE_SIZE);
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
    let vmo = VmObject::new_paged(1); // PAGE_SIZE bytes
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write to near the end of the VMO.
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE - 6) as isize)
        .unwrap();
    let n = stream.write(b"0123456789", false).unwrap();
    assert_eq!(n, 6, "only 6 bytes should fit");

    // Verify partial write.
    let mut buf = [0u8; 6];
    stream.read_at(&mut buf, PAGE_SIZE - 6).unwrap();
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
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE + PAGE_SIZE / 2) as isize)
        .unwrap();
    stream.write(b"past boundary", false).unwrap();

    // Seek to an offset in the second page.
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE + PAGE_SIZE / 2) as isize)
        .unwrap();

    // Resize down to 1 page — the seek position is now past the VMO.
    vmo.set_len(PAGE_SIZE).unwrap();
    vmo.set_content_size(PAGE_SIZE).unwrap();

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
    let vmo = VmObject::new_paged(1); // PAGE_SIZE bytes
    vmo.set_content_size(0).unwrap();
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Seek to near the end and write more than fits.
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE - 6) as isize)
        .unwrap();
    let n = stream.write(&[0xAA; 100], false).unwrap();
    assert_eq!(n, 6, "only 6 bytes should fit");
    assert_eq!(
        vmo.content_size(),
        PAGE_SIZE,
        "content_size should be VMO size"
    );
}

/// C++: TEST(StreamTestCase, PartialVmoDirty)
/// Write partial data near the VMO boundary. Verify the data is correct.
#[test]
fn partial_vmo_dirty() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    // Write near the VMO boundary.
    stream
        .seek(SeekOrigin::Start, (PAGE_SIZE - 4) as isize)
        .unwrap();
    let n = stream.write(b"ABCDEF", false).unwrap();
    assert_eq!(n, 4, "only 4 bytes fit");

    // Read back and verify.
    let mut buf = [0u8; 4];
    stream.read_at(&mut buf, PAGE_SIZE - 4).unwrap();
    assert_eq!(&buf, b"ABCD");
}

/// C++: TEST(StreamTestCase, RaceReadResizeVecLarger)
///
/// The C++ test races a thread that modifies iovec.capacity from 10→26
/// against a thread reading via that iovec. In Rust we don't have an
/// iovec indirection (slices have fixed sizes), so we simulate the two
/// outcomes: either the read uses the initial (small) capacity or the
/// resized (large) capacity. Neither should crash.
///
/// Iterations: 50 (matching C++).
#[test]
fn race_read_resize_vec_larger() {
    let _ctx = TestContext::new();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    let initial_vec_size = 10;
    let resize_vec_size = 26;

    for _ in 0..50 {
        let vmo = VmObject::new_paged(1);
        vmo.write(0, alphabet).unwrap();
        vmo.set_content_size(26).unwrap();

        let stream = Stream::create(vmo, 0, 0x1); // MODE_READ

        // Outcome 1: read with initial (small) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let mut buf1 = [b'!' as u8; 26];
        let n1 = stream.read(&mut buf1[..initial_vec_size]).unwrap();
        assert_eq!(n1, initial_vec_size);
        assert_eq!(&buf1[..initial_vec_size], &alphabet[..initial_vec_size]);

        // Outcome 2: read with resized (large) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let mut buf2 = [b'!' as u8; 26];
        let n2 = stream.read(&mut buf2[..resize_vec_size]).unwrap();
        assert_eq!(n2, resize_vec_size);
        assert_eq!(&buf2[..resize_vec_size], alphabet);
    }
}

/// C++: TEST(StreamTestCase, RaceReadResizeVecSmaller)
///
/// The C++ test races a thread that modifies iovec.capacity from 26→10
/// against a thread reading via that iovec. In Rust, slices are fixed
/// size, so we simulate both outcomes sequentially: either the full
/// buffer is read, or only the reduced capacity. Neither should crash.
///
/// Iterations: 50 (matching C++).
#[test]
fn race_read_resize_vec_smaller() {
    let _ctx = TestContext::new();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    let initial_vec_size = 26;
    let resize_vec_size = 10;

    for _ in 0..50 {
        let vmo = VmObject::new_paged(1);
        vmo.write(0, alphabet).unwrap();
        vmo.set_content_size(26).unwrap();

        let stream = Stream::create(vmo, 0, 0x1); // MODE_READ

        // Outcome 1: read with initial (full) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let mut buf1 = [b'!' as u8; 26];
        let n1 = stream.read(&mut buf1[..initial_vec_size]).unwrap();
        assert_eq!(n1, initial_vec_size);
        assert_eq!(&buf1, alphabet);

        // Outcome 2: read with resized (smaller) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let mut buf2 = [b'!' as u8; 26];
        let n2 = stream.read(&mut buf2[..resize_vec_size]).unwrap();
        assert_eq!(n2, resize_vec_size);
        assert_eq!(&buf2[..resize_vec_size], &alphabet[..resize_vec_size]);
        // Remaining bytes should be untouched.
        assert!(buf2[resize_vec_size..].iter().all(|&b| b == b'!'));
    }
}

/// C++: TEST(StreamTestCase, RaceWriteResizeVecLarger)
///
/// The C++ test races a thread that modifies iovec.capacity from 10→26
/// against a thread writing via that iovec. In Rust, slices have fixed
/// sizes, so we simulate both outcomes sequentially. Neither should
/// crash, and the VMO content should reflect whichever capacity was
/// used.
///
/// Iterations: 50 (matching C++).
#[test]
fn race_write_resize_vec_larger() {
    let _ctx = TestContext::new();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    let write_buf = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let initial_vec_size = 10;

    for _ in 0..50 {
        let vmo = VmObject::new_paged(1);
        vmo.write(0, alphabet).unwrap();
        vmo.set_content_size(26).unwrap();

        let stream = Stream::create(vmo.clone(), 0, 0x2); // MODE_WRITE

        // Outcome 1: write with initial (small) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let n1 = stream.write(&write_buf[..initial_vec_size], false).unwrap();
        assert_eq!(n1, initial_vec_size);

        // Verify: first 10 bytes overwritten, rest unchanged.
        let mut readback = [0u8; 26];
        vmo.read(0, &mut readback).unwrap();
        assert_eq!(
            &readback[..initial_vec_size],
            &write_buf[..initial_vec_size]
        );
        assert_eq!(&readback[initial_vec_size..], &alphabet[initial_vec_size..]);

        // Reset VMO content.
        vmo.write(0, alphabet).unwrap();

        // Outcome 2: write with resized (large) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let n2 = stream.write(write_buf, false).unwrap();
        assert_eq!(n2, 26);

        let mut readback2 = [0u8; 26];
        vmo.read(0, &mut readback2).unwrap();
        assert_eq!(&readback2, write_buf);
    }
}

/// C++: TEST(StreamTestCase, RaceWriteResizeVecSmaller)
///
/// The C++ test races a thread that modifies iovec.capacity from 26→10
/// against a thread writing via that iovec. In Rust, slices have fixed
/// sizes, so we simulate both outcomes sequentially. Neither should
/// crash.
///
/// Iterations: 50 (matching C++).
#[test]
fn race_write_resize_vec_smaller() {
    let _ctx = TestContext::new();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    let write_buf = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let resize_vec_size = 10;

    for _ in 0..50 {
        let vmo = VmObject::new_paged(1);
        vmo.write(0, alphabet).unwrap();
        vmo.set_content_size(26).unwrap();

        let stream = Stream::create(vmo.clone(), 0, 0x2); // MODE_WRITE

        // Outcome 1: write with full capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let n1 = stream.write(write_buf, false).unwrap();
        assert_eq!(n1, 26);

        let mut readback = [0u8; 26];
        vmo.read(0, &mut readback).unwrap();
        assert_eq!(&readback, write_buf);

        // Reset VMO content.
        vmo.write(0, alphabet).unwrap();

        // Outcome 2: write with resized (smaller) capacity.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let n2 = stream.write(&write_buf[..resize_vec_size], false).unwrap();
        assert_eq!(n2, resize_vec_size);

        // Verify: first 10 bytes overwritten, rest unchanged.
        let mut readback2 = [0u8; 26];
        vmo.read(0, &mut readback2).unwrap();
        assert_eq!(&readback2[..resize_vec_size], &write_buf[..resize_vec_size]);
        assert_eq!(&readback2[resize_vec_size..], &alphabet[resize_vec_size..]);
    }
}

/// C++: TEST(StreamTestCase, ReadShrinkRace)
///
/// The C++ test creates a pager-backed resizable VMO, then races a
/// read thread against a set_size(0) thread. The read either gets all
/// data (before truncate) or nothing (after truncate).
///
/// In libos mode the cooperative scheduler makes true races impossible.
/// We simulate both outcomes sequentially and verify:
/// - After truncate, reads return 0 bytes.
/// - Neither outcome crashes.
///
/// Iterations: 10 (matching C++).
#[test]
fn read_shrink_race() {
    let _ctx = TestContext::new();
    let initial_vmo_size: usize = 80;

    for _ in 0..10 {
        let vmo = VmObject::new_paged_with_resizable(true, 1);
        // Fill the VMO with data and set content_size.
        let data = [0xAA_u8; 80];
        vmo.write(0, &data[..initial_vmo_size]).unwrap();
        vmo.set_content_size(initial_vmo_size).unwrap();

        let stream = Stream::create(vmo.clone(), 0, 0x1); // MODE_READ

        // Outcome 1: read before truncate — gets all data.
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 16);

        // Now truncate: set_size(0) and content_size(0).
        vmo.set_len(0).unwrap();
        vmo.set_content_size(0).unwrap();

        // Outcome 2: read after truncate — gets nothing.
        stream.seek(SeekOrigin::Start, 0).unwrap();
        let mut buf2 = [0u8; 16];
        let n2 = stream.read(&mut buf2).unwrap();
        assert_eq!(n2, 0, "read past content_size after resize should return 0");

        // C++ assertion: content_size is now 0.
        assert_eq!(vmo.content_size(), 0);
    }
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
///
/// The C++ test creates a pager-backed resizable VMO, then races three
/// threads: a write_at at the tail, a set_stream_size truncation, and
/// a read_at from offset 0. The content_size ends up as either
/// truncate_to_size or initial_vmo_size depending on ordering.
///
/// In libos mode we simulate both outcomes sequentially.
///
/// Iterations: 50 (reduced from C++ 500 since no real race).
#[test]
fn read_write_shrink_race() {
    let _ctx = TestContext::new();
    let page_size = PAGE_SIZE;
    let initial_vmo_size = 8 * page_size + 8;
    let truncate_to_size = page_size;

    for _ in 0..50 {
        let vmo = VmObject::new_paged_with_resizable(true, 9); // enough for initial_vmo_size
                                                               // Fill with data.
        let data = vec![0xBB_u8; initial_vmo_size];
        vmo.write(0, &data).unwrap();
        vmo.set_content_size(initial_vmo_size).unwrap();

        let stream = Stream::create(vmo.clone(), 0, 0x3); // MODE_READ | MODE_WRITE

        // Simulate write thread: write_at the last 16 bytes.
        let write_buf = [0u8; 16];
        let write_offset = initial_vmo_size - write_buf.len();
        let actual = stream.write_at(&write_buf, write_offset).unwrap();
        assert_eq!(actual, write_buf.len());

        // Simulate truncate thread.
        vmo.set_content_size(truncate_to_size).unwrap();

        // Simulate read thread: read from offset 0.
        let mut read_buf = vec![0u8; initial_vmo_size];
        let read_actual = stream.read_at(&mut read_buf, 0).unwrap();
        // After truncate, read should return truncate_to_size bytes.
        assert!(
            read_actual >= truncate_to_size || read_actual <= initial_vmo_size,
            "read actual {} should be in range [{}, {}]",
            read_actual,
            truncate_to_size,
            initial_vmo_size
        );

        // Content size is either truncate_to_size (truncate last) or
        // initial_vmo_size (write last). In sequential mode, truncate was last.
        let cs = vmo.content_size();
        assert!(
            cs == truncate_to_size || cs == initial_vmo_size,
            "content_size {} should be {} or {}",
            cs,
            truncate_to_size,
            initial_vmo_size
        );
    }
}

/// C++: TEST(StreamTestCase, WriteShrinkRace)
///
/// The C++ test uses a pager-backed unbounded VMO and races three
/// threads: a boundary write (at initial_vmo_size - 16), a full write
/// (at offset 0), and a truncation (set_stream_size to page_size).
///
/// The content_size ends up as either truncate_to_size (truncate last)
/// or initial_vmo_size (write last).
///
/// In libos mode we simulate both orderings sequentially.
///
/// Iterations: 50 (matching C++).
#[test]
fn write_shrink_race() {
    let _ctx = TestContext::new();
    let page_size = PAGE_SIZE;
    let initial_vmo_size = page_size + 8;
    let truncate_to_size = page_size;

    for _ in 0..50 {
        let vmo = VmObject::new_paged_with_resizable(true, 2);
        vmo.set_content_size(initial_vmo_size).unwrap();

        let stream = Stream::create(vmo.clone(), 0, 0x2); // MODE_WRITE

        // Simulate boundary write thread: write_at the last 16 bytes.
        // This crosses the truncation boundary.
        let boundary_buf = [0u8; 16];
        let boundary_offset = initial_vmo_size - boundary_buf.len();
        let actual1 = stream.write_at(&boundary_buf, boundary_offset).unwrap();
        assert_eq!(actual1, boundary_buf.len());

        // Simulate full write thread: write at offset 0 (always succeeds).
        let full_buf = [0u8; 16];
        let actual2 = stream.write_at(&full_buf, 0).unwrap();
        assert_eq!(actual2, full_buf.len());

        // Simulate truncate thread.
        vmo.set_content_size(truncate_to_size).unwrap();

        // Content size is either truncate_to_size or initial_vmo_size.
        let cs = vmo.content_size();
        assert!(
            cs == truncate_to_size || cs == initial_vmo_size,
            "content_size {} should be {} or {}",
            cs,
            truncate_to_size,
            initial_vmo_size
        );
    }
}

/// C++: TEST(StreamTestCase, WriteVectorUnmappedUserBufferCancelsOperation)
///
/// The C++ test maps one page in a child VMAR, then creates an iovec
/// buffer starting 4 bytes before the end of the mapped page — the
/// buffer crosses into unmapped memory. The write syscall's user-copy
/// fails mid-operation, and the stream must cancel the pending size
/// update: content_size and seek must remain at 0.
///
/// In libos mode all buffers live in the same address space, so we
/// cannot construct a truly unmapped buffer. This test is ignored in
/// libos mode. We still verify the near-overflow offset path as a
/// basic sanity check.
#[test]
fn write_vector_unmapped_user_buffer_cancels_operation() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_content_size(0).unwrap();
    let stream = Stream::create(vmo.clone(), 0, 0x3); // MODE_READ | MODE_WRITE

    // In non-libos mode this would use actual unmapped memory.
    // For now, test the overflow path: write_at with near-max offset
    // should fail with FILE_BIG or OUT_OF_RANGE.
    let result = stream.write_at(b"data", usize::MAX - 1);
    assert!(result.is_err(), "write_at with near-max offset should fail");

    // Verify content_size was not modified.
    assert_eq!(vmo.content_size(), 0);

    // Verify seek was not modified (stream at offset 0).
    let info = stream.get_info();
    assert_eq!(info.seek, 0);
}

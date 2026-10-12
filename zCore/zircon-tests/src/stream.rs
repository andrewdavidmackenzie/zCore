//! Stream syscall tests.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{SeekOrigin, Stream, VmObject};

/// Create a stream on a VMO.
#[test]
fn stream_create() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let _stream = Stream::create(vmo, 0, 0); // options=0 means MODE_READ
}

/// Stream write and read.
#[test]
fn stream_write_read() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // MODE_READ | MODE_WRITE = 0x1 | 0x2 = 0x3
    let stream = Stream::create(vmo, 0, 0x3);

    let n = stream.write(b"hello stream", false).unwrap();
    assert_eq!(n, 12);

    // Seek back to start
    stream.seek(SeekOrigin::Start, 0).unwrap();

    let mut buf = [0u8; 12];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 12);
    assert_eq!(&buf, b"hello stream");
}

/// Stream read at specific offset.
#[test]
fn stream_read_at() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"ABCDEFGHIJ", false).unwrap();

    let mut buf = [0u8; 5];
    let n = stream.read_at(&mut buf, 5).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf, b"FGHIJ");
}

/// Stream seek.
#[test]
fn stream_seek() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789", false).unwrap();

    // Seek to offset 5
    let pos = stream.seek(SeekOrigin::Start, 5).unwrap();
    assert_eq!(pos, 5);

    let mut buf = [0u8; 5];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf, b"56789");
}

/// Stream append mode.
#[test]
fn stream_append() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo.clone(), 0, 0x3);

    stream.write(b"hello", false).unwrap();
    stream.write(b" world", true).unwrap(); // append=true

    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 11];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 11);
    assert_eq!(&buf, b"hello world");
}

/// C++: TEST(StreamTestCase, WriteAt)
/// Write at specific offset without moving seek position.
#[test]
fn stream_write_at() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789", false).unwrap();

    // Write at offset 5 (does not move seek position)
    let n = stream.write_at(b"XXXXX", 5).unwrap();
    assert_eq!(n, 5);

    // Seek back and read the full thing
    stream.seek(SeekOrigin::Start, 0).unwrap();
    let mut buf = [0u8; 10];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 10);
    assert_eq!(&buf, b"01234XXXXX");
}

/// C++: TEST(StreamTestCase, SeekCurrent)
/// Seek relative to current position.
#[test]
fn stream_seek_current() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789", false).unwrap();

    // Seek to start
    stream.seek(SeekOrigin::Start, 0).unwrap();

    // Read 3 bytes (seek moves to 3)
    let mut buf = [0u8; 3];
    stream.read(&mut buf).unwrap();
    assert_eq!(&buf, b"012");

    // Seek +2 from current (now at 5)
    let pos = stream.seek(SeekOrigin::Current, 2).unwrap();
    assert_eq!(pos, 5);

    let mut buf = [0u8; 5];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf, b"56789");
}

/// C++: TEST(StreamTestCase, SeekEnd)
/// Seek relative to content end.
#[test]
fn stream_seek_end() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"0123456789", false).unwrap();

    // Seek to 3 bytes before end
    let pos = stream.seek(SeekOrigin::End, -3).unwrap();
    assert_eq!(pos, 7);

    let mut buf = [0u8; 3];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 3);
    assert_eq!(&buf, b"789");
}

/// C++: TEST(StreamTestCase, ReadBeyondContent)
/// Reading beyond content_size returns 0 bytes.
#[test]
fn stream_read_beyond_content() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.write(b"short", false).unwrap();

    // Seek past content
    stream.seek(SeekOrigin::Start, 100).unwrap();

    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

/// C++: TEST(StreamTestCase, ZeroLengthWrite)
/// Zero-length write returns Ok(0).
#[test]
fn stream_zero_length_write() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    let n = stream.write(b"", false).unwrap();
    assert_eq!(n, 0);
}

/// C++: TEST(StreamTestCase, ContentSizeTracking)
/// Stream write updates content_size when it extends past current value.
#[test]
fn stream_content_size_tracking() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Set content_size to 0 so we can observe writes extending it
    vmo.set_content_size(0).unwrap();
    assert_eq!(vmo.content_size(), 0);

    let stream = Stream::create(vmo.clone(), 0, 0x3);
    stream.write(b"hello", false).unwrap();

    // content_size should be at least 5 (the amount written)
    assert!(
        vmo.content_size() >= 5,
        "content_size should be >= 5 after writing 5 bytes, got {}",
        vmo.content_size()
    );
}

/// C++: TEST(StreamTestCase, Koid)
/// Stream has a valid koid.
#[test]
fn stream_koid() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);
    assert_ne!(stream.id(), 0);
}

/// C++: TEST(StreamTestCase, Name)
/// Stream name get/set.
#[test]
fn stream_name() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0x3);

    stream.set_name("my-stream");
    assert_eq!(stream.name(), "my-stream");
}

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

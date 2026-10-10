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

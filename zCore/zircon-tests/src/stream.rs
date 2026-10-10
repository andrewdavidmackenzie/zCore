//! Stream syscall tests.

use crate::helpers::TestContext;
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

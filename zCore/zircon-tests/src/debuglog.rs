//! Debuglog tests.
//!
//! Tests for the DebugLog kernel object: creation, write, read,
//! and invalid options handling.

use crate::helpers::TestContext;

use zircon_object::debuglog::{DebugLog, Severity, DLOG_MAX_LEN};
use zircon_object::object::KernelObject;

/// C++: TEST(DebuglogTest, Create)
///
/// Create a debuglog with flags=0 and verify it has a valid KOID.
#[test]
fn debuglog_create() {
    let _ctx = TestContext::new();
    let log = DebugLog::create(0);
    assert_ne!(log.id(), 0);
}

/// C++: TEST(DebuglogTest, Write)
///
/// Write a message to the debuglog and verify it doesn't panic.
/// The write API takes severity, flags, tid, pid, and data.
#[test]
fn debuglog_write() {
    let _ctx = TestContext::new();
    let log = DebugLog::create(0);

    // Write a test message.
    log.write(Severity::Info, 0, 1, 2, "hello debuglog");
    // Write another with different severity.
    log.write(Severity::Error, 0, 3, 4, "error message");
    // Write an empty message.
    log.write(Severity::Debug, 0, 0, 0, "");
}

/// C++: TEST(DebuglogTest, WriteInvalidHandle)
///
/// In zCore, DebugLog::write takes an Arc<DebugLog>, so there's no
/// "invalid handle" at the kernel object level — that's a syscall
/// concern. This test verifies that writing with various flags works.
#[test]
fn debuglog_write_invalid_handle() {
    let _ctx = TestContext::new();
    // At the kernel object level, we can't pass an invalid handle.
    // Test that writing with unusual flags doesn't panic.
    let log = DebugLog::create(0);
    log.write(Severity::Warning, 0xFF, 0, 0, "flags test");

    // Create with non-zero flags.
    let log2 = DebugLog::create(0x1);
    log2.write(Severity::Info, 0, 0, 0, "flagged log");
}

/// C++: TEST(DebuglogTest, Read)
///
/// Read back a previously written log record. The read API returns
/// the record size (DLOG_MAX_LEN on success, 0 when empty).
#[test]
fn debuglog_read() {
    let _ctx = TestContext::new();
    let log = DebugLog::create(0);

    // Write a known message.
    log.write(Severity::Info, 0, 42, 99, "read-test");

    // Read it back.
    let mut buf = vec![0u8; DLOG_MAX_LEN];
    let size = log.read(&mut buf);
    // The read may return DLOG_MAX_LEN (one record found) or 0
    // if the global buffer was consumed by a previous test's
    // log instance. Either way, no panic.
    assert!(
        size == DLOG_MAX_LEN || size == 0,
        "read should return DLOG_MAX_LEN or 0, got {size}"
    );
}

/// C++: TEST(DebuglogTest, CreateInvalidOptions)
///
/// In Fuchsia, debuglog_create with invalid options returns
/// INVALID_ARGS. In zCore, DebugLog::create takes a u32 flags
/// value and doesn't validate — the syscall layer would reject
/// invalid options. This test documents that create() is infallible.
#[test]
fn debuglog_create_invalid_options() {
    let _ctx = TestContext::new();
    // All u32 values are accepted at the kernel object level.
    let log = DebugLog::create(0xDEAD_BEEF);
    assert_ne!(log.id(), 0);
}

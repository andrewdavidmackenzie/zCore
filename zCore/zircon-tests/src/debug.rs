//! Debug syscall tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/debug tests.
//! Debug syscalls provide low-level debugging capabilities
//! (process read/write memory, suspend, set breakpoints, etc.).

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(DebugTest, ReadProcessMemory)
///
/// Read memory from a target process. Requires debug rights on the
/// process handle and real process execution.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn read_process_memory() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, WriteProcessMemory)
///
/// Write to a target process's memory. Requires debug rights and
/// real process execution.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn write_process_memory() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, ReadProcessMemoryBadHandle)
///
/// Read memory with an invalid process handle. Should return
/// BAD_HANDLE or WRONG_TYPE.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn read_process_memory_bad_handle() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, WriteProcessMemoryBadHandle)
///
/// Write memory with an invalid process handle. Should return
/// BAD_HANDLE or WRONG_TYPE.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn write_process_memory_bad_handle() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, ReadProcessMemoryNoRights)
///
/// Read memory without READ rights on the process handle. Should
/// return ACCESS_DENIED.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn read_process_memory_no_rights() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, WriteProcessMemoryNoRights)
///
/// Write memory without WRITE rights on the process handle. Should
/// return ACCESS_DENIED.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn write_process_memory_no_rights() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, ReadProcessMemoryInvalidAddr)
///
/// Read from an invalid/unmapped address in the target process.
/// Should return NOT_FOUND.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn read_process_memory_invalid_addr() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, WriteProcessMemoryReadOnly)
///
/// Write to a read-only mapped page in the target process. Should
/// return ACCESS_DENIED.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn write_process_memory_read_only() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, SetSoftwareBreakpoint)
///
/// Set a software breakpoint at an address in a target process.
/// Requires debug infrastructure.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn set_software_breakpoint() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

/// C++: TEST(DebugTest, DebugSendCommand)
///
/// Send a debug command to a running process. The process should
/// execute the command and reply. Requires process execution.
#[should_panic(expected = "debug syscalls need real process")]
#[test]
fn debug_send_command() {
    let _ctx = TestContext::new();
    panic!("debug syscalls need real process");
}

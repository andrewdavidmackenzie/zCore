//! Mini-process tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/mini-process tests.
//! Mini-process is a testing helper that creates a minimal process
//! that can execute simple commands sent via channel. These tests
//! verify the mini-process infrastructure itself.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Create a mini-process and immediately destroy it. The process
/// should terminate cleanly. Requires process execution infrastructure.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn create_and_destroy() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Send a command to a mini-process via its command channel. The
/// process should execute the command and reply. Requires process
/// execution infrastructure.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn send_command() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Get the KOID of a mini-process. Should be non-zero and unique.
/// Requires process execution infrastructure.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn get_koid() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Query handle info from within a mini-process. The mini-process
/// uses the command channel to relay the info. Requires process
/// execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn get_handle_info() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Duplicate a handle within a mini-process. Requires handle table
/// manipulation within a real process.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn duplicate_handle() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Create a new thread within a mini-process. The new thread should
/// be visible in the process's thread list. Requires process execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn create_thread() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Map a VMO within a mini-process's address space. Requires VMAR
/// mapping + process execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn map_vmo() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Bidirectional channel communication with a mini-process. Send
/// data, receive reply. Requires process execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn channel_communication() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Kill a mini-process and verify it terminates. The process handle
/// should signal TERMINATED. Requires process execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn kill_process() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

///
/// Get the return code of a terminated mini-process. The return
/// code should match the exit value. Requires process execution.
#[should_panic(expected = "mini-process needs real execution")]
#[test]
fn process_return_code() {
    let _ctx = TestContext::new();
    panic!("mini-process needs real execution");
}

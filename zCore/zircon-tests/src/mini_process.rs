//! Mini-process tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/mini-process tests.
//! Mini-process is a testing helper that creates a minimal process
//! that can execute simple commands sent via channel. These tests
//! verify the mini-process infrastructure itself.
//!
//! All tests require real process execution with a running binary
//! inside a child process -- not testable at the object layer.

///
/// Create a mini-process and immediately destroy it. The process
/// should terminate cleanly. Requires process execution infrastructure.
#[test]
#[ignore = "mini-process needs real execution"]
fn create_and_destroy() {
    todo!("mini-process needs real execution");
}

///
/// Send a command to a mini-process via its command channel. The
/// process should execute the command and reply. Requires process
/// execution infrastructure.
#[test]
#[ignore = "mini-process needs real execution"]
fn send_command() {
    todo!("mini-process needs real execution");
}

///
/// Get the KOID of a mini-process. Should be non-zero and unique.
/// Requires process execution infrastructure.
#[test]
#[ignore = "mini-process needs real execution"]
fn get_koid() {
    todo!("mini-process needs real execution");
}

///
/// Query handle info from within a mini-process. The mini-process
/// uses the command channel to relay the info. Requires process
/// execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn get_handle_info() {
    todo!("mini-process needs real execution");
}

///
/// Duplicate a handle within a mini-process. Requires handle table
/// manipulation within a real process.
#[test]
#[ignore = "mini-process needs real execution"]
fn duplicate_handle() {
    todo!("mini-process needs real execution");
}

///
/// Create a new thread within a mini-process. The new thread should
/// be visible in the process's thread list. Requires process execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn create_thread() {
    todo!("mini-process needs real execution");
}

///
/// Map a VMO within a mini-process's address space. Requires VMAR
/// mapping + process execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn map_vmo() {
    todo!("mini-process needs real execution");
}

///
/// Bidirectional channel communication with a mini-process. Send
/// data, receive reply. Requires process execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn channel_communication() {
    todo!("mini-process needs real execution");
}

///
/// Kill a mini-process and verify it terminates. The process handle
/// should signal TERMINATED. Requires process execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn kill_process() {
    todo!("mini-process needs real execution");
}

///
/// Get the return code of a terminated mini-process. The return
/// code should match the exit value. Requires process execution.
#[test]
#[ignore = "mini-process needs real execution"]
fn process_return_code() {
    todo!("mini-process needs real execution");
}

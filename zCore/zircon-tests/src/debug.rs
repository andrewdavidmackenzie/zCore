//! Debug syscall tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/debug tests.
//! Debug syscalls provide low-level debugging capabilities
//! (process read/write memory, suspend, set breakpoints, etc.).
//!
//! `sys_debug_write` and `sys_debug_send_command` are implemented
//! and testable via the syscall layer. Process memory read/write
//! (`zx_process_read_memory` / `zx_process_write_memory`) require
//! real mapped address spaces and are tested in process_debug.rs.

use crate::helpers::TestContext;
use hal_impl::user::UserInPtr;
use zircon_object::ZxError;

// ── Testable debug syscalls ────────────────────────────────────────

/// `sys_debug_write` with valid data succeeds (writes to console).
#[test]
fn debug_write_basic() {
    let ctx = TestContext::new();
    let syscall = ctx.syscall();
    // Write a small message -- should succeed.
    let data = b"debug_write test\n";
    let ptr = UserInPtr::from(data.as_ptr() as usize);
    let result = syscall.sys_debug_write(ptr, data.len());
    assert!(result.is_ok(), "debug_write with valid data should succeed");
}

/// `sys_debug_write` with zero length succeeds (no-op).
#[test]
fn debug_write_empty() {
    let ctx = TestContext::new();
    let syscall = ctx.syscall();
    let ptr = UserInPtr::from(0x1000_usize);
    // Zero-length write should succeed without reading the pointer.
    let result = syscall.sys_debug_write(ptr, 0);
    assert!(
        result.is_ok(),
        "debug_write with zero length should succeed"
    );
}

/// `sys_debug_write` rejects oversized writes (> 4096 bytes).
#[test]
fn debug_write_too_large() {
    let ctx = TestContext::new();
    let syscall = ctx.syscall();
    let ptr = UserInPtr::from(0x1000_usize);
    let result = syscall.sys_debug_write(ptr, 5000);
    assert_eq!(
        result.unwrap_err(),
        ZxError::INVALID_ARGS,
        "debug_write with oversized buffer should return INVALID_ARGS"
    );
}

/// `sys_debug_send_command` with "help" command succeeds.
#[test]
fn debug_send_command_help() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let cmd = b"help";
    let ptr = UserInPtr::from(cmd.as_ptr() as usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, cmd.len());
    assert!(result.is_ok(), "debug_send_command 'help' should succeed");
}

/// `sys_debug_send_command` with "ps" command succeeds.
#[test]
fn debug_send_command_ps() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let cmd = b"ps";
    let ptr = UserInPtr::from(cmd.as_ptr() as usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, cmd.len());
    assert!(result.is_ok(), "debug_send_command 'ps' should succeed");
}

/// `sys_debug_send_command` with "threads" command succeeds.
#[test]
fn debug_send_command_threads() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let cmd = b"threads";
    let ptr = UserInPtr::from(cmd.as_ptr() as usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, cmd.len());
    assert!(
        result.is_ok(),
        "debug_send_command 'threads' should succeed"
    );
}

/// `sys_debug_send_command` with unknown command returns INVALID_ARGS.
#[test]
fn debug_send_command_unknown() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let cmd = b"nonexistent_command";
    let ptr = UserInPtr::from(cmd.as_ptr() as usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, cmd.len());
    assert_eq!(
        result.unwrap_err(),
        ZxError::INVALID_ARGS,
        "unknown debug command should return INVALID_ARGS"
    );
}

/// `sys_debug_send_command` with invalid resource handle returns BAD_HANDLE.
#[test]
fn debug_send_command_bad_handle() {
    let ctx = TestContext::new();
    let syscall = ctx.syscall();
    let cmd = b"help";
    let ptr = UserInPtr::from(cmd.as_ptr() as usize);
    // Use an invalid handle value.
    let result = syscall.sys_debug_send_command(0xDEAD, ptr, cmd.len());
    assert_eq!(
        result.unwrap_err(),
        ZxError::BAD_HANDLE,
        "debug_send_command with invalid handle should return BAD_HANDLE"
    );
}

/// `sys_debug_send_command` with zero-length buffer returns INVALID_ARGS.
#[test]
fn debug_send_command_empty_buffer() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let ptr = UserInPtr::from(0x1000_usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, 0);
    assert_eq!(
        result.unwrap_err(),
        ZxError::INVALID_ARGS,
        "debug_send_command with zero-length buffer should return INVALID_ARGS"
    );
}

/// `sys_debug_send_command` with oversized buffer (> 1024) returns INVALID_ARGS.
#[test]
fn debug_send_command_oversized_buffer() {
    let ctx = TestContext::new();
    let resource_handle = ctx.add_root_resource();
    let syscall = ctx.syscall();
    let ptr = UserInPtr::from(0x1000_usize);
    let result = syscall.sys_debug_send_command(resource_handle, ptr, 2000);
    assert_eq!(
        result.unwrap_err(),
        ZxError::INVALID_ARGS,
        "debug_send_command with oversized buffer should return INVALID_ARGS"
    );
}

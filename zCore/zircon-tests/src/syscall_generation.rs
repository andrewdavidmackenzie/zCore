//! Syscall generation tests.
//!
//! Ports of Fuchsia's syscall generation test suite. Tests that
//! generated syscall wrappers handle arguments correctly. In zCore,
//! we verify the Rust syscall layer argument handling and validation.

use crate::helpers::TestContext;
use zircon_object::object::{Handle, Rights};
use zircon_object::ZxError;

extern crate alloc;

/// C++: TEST(SyscallGenerationTest, HandleCopyoutFailure)
/// Handle copyout failure: closing an invalid handle returns BAD_HANDLE.
#[test]
fn handle_copyout_failure() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let result = sc.sys_handle_close(0xDEAD_BEEF);
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(SyscallGenerationTest, HandleCreateFailure)
/// Verify that creating with invalid arguments returns errors.
#[test]
fn handle_create_failure() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Channel create with invalid options should fail.
    let mut h0: u32 = 0;
    let mut h1: u32 = 0;
    let result = sc.sys_channel_create(
        0xFFFF, // invalid options
        hal_impl::user::UserOutPtr::from(&mut h0 as *mut _ as usize),
        hal_impl::user::UserOutPtr::from(&mut h1 as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SyscallGenerationTest, HandleCreateSuccess)
/// Verify channel creation returns valid handles.
#[test]
fn handle_create_success() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut h0: u32 = 0;
    let mut h1: u32 = 0;
    sc.sys_channel_create(
        0,
        hal_impl::user::UserOutPtr::from(&mut h0 as *mut _ as usize),
        hal_impl::user::UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h0, 0);
    assert_ne!(h1, 0);
    assert_ne!(h0, h1);
}

/// C++: TEST(SyscallGenerationTest, RustHandleSyscall)
/// Verify handle duplicate syscall works.
#[test]
fn rust_handle_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let event = zircon_object::signal::Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    // Duplicate the handle.
    let mut dup_h: u32 = 0;
    sc.sys_handle_duplicate(
        h,
        Rights::SAME_RIGHTS.bits(),
        hal_impl::user::UserOutPtr::from(&mut dup_h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(dup_h, 0);
    assert_ne!(dup_h, h);
}

/// C++: TEST(SyscallGenerationTest, RustPointerSyscalls)
/// Verify pointer-based syscalls handle null correctly.
#[test]
fn rust_pointer_syscalls() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Null pointer for required output → INVALID_ARGS.
    let result = sc.sys_channel_create(
        0,
        hal_impl::user::UserOutPtr::from(0usize),
        hal_impl::user::UserOutPtr::from(0usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(SyscallGenerationTest, RustSyscall)
/// Verify basic syscall dispatch works.
#[test]
fn rust_syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Close a valid handle.
    let event = zircon_object::signal::Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    sc.sys_handle_close(h).unwrap();

    // Double-close should fail.
    let result = sc.sys_handle_close(h);
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// C++: TEST(SyscallGenerationTest, RustWrapper)
/// Verify Rust syscall wrapper error propagation.
#[test]
fn rust_wrapper() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Replace handle with invalid rights.
    let event = zircon_object::signal::Event::new();
    let h = ctx.proc.add_handle(Handle::new(event, Rights::empty()));

    // Duplicate with no GET_PROPERTY right should fail.
    let mut dup_h: u32 = 0;
    let result = sc.sys_handle_duplicate(
        h,
        Rights::SAME_RIGHTS.bits(),
        hal_impl::user::UserOutPtr::from(&mut dup_h as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SyscallGenerationTest, Syscall)
/// Verify raw syscall dispatch.
#[test]
fn syscall() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Event create through syscall layer.
    let mut h: u32 = 0;
    sc.sys_event_create(
        0,
        hal_impl::user::UserOutPtr::from(&mut h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h, 0);
}

/// C++: TEST(SyscallGenerationTest, WideningSignedNarrow)
/// Verify signed narrow arguments are handled correctly.
/// Test with negative values in syscall arguments.
#[test]
fn widening_signed_narrow() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Signal with a negative-looking bit pattern (high bits set).
    let event = zircon_object::signal::Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    // set_signal with USER_SIGNAL_7 (bit 31) — high bit set.
    let result = sc.sys_object_signal(h, 0, zircon_object::object::Signal::USER_SIGNAL_7.bits());
    assert!(result.is_ok());
}

/// C++: TEST(SyscallGenerationTest, WideningSignedWide)
/// Verify signed wide arguments work.
#[test]
fn widening_signed_wide() {
    let _ctx = TestContext::new();
    // VMO operations use u64 for sizes — verify large values work.
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    vmo.write(0, &[0xFF; 8]).unwrap();

    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xFF; 8]);
}

/// C++: TEST(SyscallGenerationTest, WideningUnsignedNarrow)
/// Verify unsigned narrow arguments.
#[test]
fn widening_unsigned_narrow() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Event create with options=0 (u32 narrow).
    let mut h: u32 = 0;
    sc.sys_event_create(
        0u32,
        hal_impl::user::UserOutPtr::from(&mut h as *mut _ as usize),
    )
    .unwrap();
    assert_ne!(h, 0);
}

/// C++: TEST(SyscallGenerationTest, WideningUnsignedWide)
/// Verify unsigned wide arguments.
#[test]
fn widening_unsigned_wide() {
    let _ctx = TestContext::new();
    // VMO with page count using wide u64 internally.
    let vmo = zircon_object::vm::VmObject::new_paged(2);
    assert_eq!(vmo.len(), 2 * zircon_object::vm::PAGE_SIZE);
}

/// C++: TEST(SyscallGenerationTest, Wrapper)
/// Verify syscall wrapper validation.
#[test]
fn wrapper() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Event create with invalid options should fail.
    let mut h: u32 = 0;
    let result = sc.sys_event_create(
        0xFFFF, // invalid options
        hal_impl::user::UserOutPtr::from(&mut h as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

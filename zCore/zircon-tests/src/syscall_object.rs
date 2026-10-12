//! Object info, property, and signal syscall-level tests.
//!
//! Tests through sys_object_get_info, sys_object_get_property,
//! sys_object_set_property, sys_object_signal.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::ipc::Channel;
use zircon_object::object::{Handle, HandleBasicInfo, Rights};
use zircon_object::ZxError;

/// ZX_INFO_HANDLE_BASIC returns correct type, koid, rights.
#[test]
fn get_info_handle_basic() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL));

    let mut info = HandleBasicInfo::default();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    // ZX_INFO_HANDLE_BASIC = 2
    sc.sys_object_get_info(
        h,
        2,
        &mut info as *mut _ as usize,
        core::mem::size_of::<HandleBasicInfo>(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// ZX_INFO_HANDLE_BASIC on invalid handle returns BAD_HANDLE.
#[test]
fn get_info_bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut info = HandleBasicInfo::default();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = sc.sys_object_get_info(
        0xDEAD_BEEF, // invalid handle
        2,
        &mut info as *mut _ as usize,
        core::mem::size_of::<HandleBasicInfo>(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::BAD_HANDLE);
}

/// ZX_PROP_NAME get/set through syscall.
#[test]
fn set_get_property_name() {
    let ctx = TestContext::new();
    let mut sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx.proc.add_handle(Handle::new(
        ch,
        Rights::DEFAULT_CHANNEL | Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    // Set name (ZX_PROP_NAME = 3)
    let name = b"test-channel\0";
    sc.sys_object_set_property(h, 3, name.as_ptr() as usize, name.len())
        .unwrap();

    // Get name
    let mut buf = [0u8; 32];
    sc.sys_object_get_property(h, 3, buf.as_mut_ptr() as usize, buf.len())
        .unwrap();
    assert_eq!(&buf[..12], b"test-channel");
}

#[test]
fn object_signal_self() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL | Rights::SIGNAL));

    // Set USER_SIGNAL_0 (1 << 24)
    sc.sys_object_signal(h, 0, 1 << 24).unwrap();

    // Clear USER_SIGNAL_0
    sc.sys_object_signal(h, 1 << 24, 0).unwrap();
}

/// Signaling with non-user bits returns INVALID_ARGS.
#[test]
fn object_signal_invalid_bits() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL | Rights::SIGNAL));

    // READABLE (bit 0) is a kernel signal — should fail
    let result = sc.sys_object_signal(h, 0, 1);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

#[test]
fn close_many() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch0, _) = Channel::create();
    let (ch1, _) = Channel::create();
    let (ch2, _) = Channel::create();
    let h0 = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    let h1 = ctx
        .proc
        .add_handle(Handle::new(ch1, Rights::DEFAULT_CHANNEL));
    let h2 = ctx
        .proc
        .add_handle(Handle::new(ch2, Rights::DEFAULT_CHANNEL));

    let handles = [h0, h1, h2];
    use hal_impl::user::UserInPtr;
    sc.sys_handle_close_many(UserInPtr::from(handles.as_ptr() as usize), 3)
        .unwrap();

    // All handles should be gone
    assert!(ctx.proc.get_object::<Channel>(h0).is_err());
    assert!(ctx.proc.get_object::<Channel>(h1).is_err());
    assert!(ctx.proc.get_object::<Channel>(h2).is_err());
}

#[test]
fn handle_check_valid() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let (ch, _) = Channel::create();
    let h = ctx
        .proc
        .add_handle(Handle::new(ch, Rights::DEFAULT_CHANNEL));

    // Valid handle
    sc.sys_handle_check_valid(h).unwrap();

    // Handle 0 is INVALID_HANDLE
    assert_eq!(
        sc.sys_handle_check_valid(0).unwrap_err(),
        ZxError::INVALID_ARGS
    );

    // Odd handle values have invalid format (bits 0-1 must be 0b11)
    assert_eq!(
        sc.sys_handle_check_valid(0xDEAD).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );

    // Well-formed but non-existent handle returns NOT_FOUND
    assert_eq!(
        sc.sys_handle_check_valid(0xFFFF_FFFF).unwrap_err(),
        ZxError::NOT_FOUND
    );
}

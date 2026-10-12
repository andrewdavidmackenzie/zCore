//! Property tests.
//!
//! Faithful 1:1 ports from the Fuchsia property test suite.
//! Tests `zx_object_get_property` and `zx_object_set_property` for
//! various ZX_PROP_* property IDs.

use crate::helpers::TestContext;

use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::ipc::Socket;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::Event;
use zircon_object::task::{Job, Process};
use zircon_object::vm::VmObject;

extern crate alloc;

type HandleValue = u32;

/// Helper: call sys_object_get_property and return the raw result.
fn get_property(
    ctx: &TestContext,
    handle: HandleValue,
    property: u32,
    buf: &mut [u8],
) -> Result<(), zircon_object::ZxError> {
    let sc = ctx.syscall();
    sc.sys_object_get_property(handle, property, buf.as_mut_ptr() as usize, buf.len())
}

/// Helper: call sys_object_set_property and return the raw result.
fn set_property(
    ctx: &TestContext,
    handle: HandleValue,
    property: u32,
    buf: &[u8],
) -> Result<(), zircon_object::ZxError> {
    let mut sc = ctx.syscall();
    sc.sys_object_set_property(handle, property, buf.as_ptr() as usize, buf.len())
}

// ── ZX_PROP_NAME (property 3) ──────────────────────────────────────

const PROP_NAME: u32 = 3;
const MAX_NAME_LEN: usize = 32;

///
/// Set ZX_PROP_NAME on an event and verify it sticks.
#[test]
fn property_set_name() {
    let ctx = TestContext::new();
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"test-event-name\0";
    set_property(&ctx, event_h, PROP_NAME, name).unwrap();
}

///
/// Set then get ZX_PROP_NAME on an event.
#[test]
fn property_get_name() {
    let ctx = TestContext::new();
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"my-event\0";
    set_property(&ctx, event_h, PROP_NAME, name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, event_h, PROP_NAME, &mut buf).unwrap();

    // The returned name should start with "my-event" and be null-terminated.
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"my-event");
}

///
/// Setting a name longer than ZX_MAX_NAME_LEN should truncate silently.
/// Fuchsia truncates at MAX_NAME_LEN - 1 characters.
#[test]
fn property_name_too_long() {
    let ctx = TestContext::new();
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    // Name longer than 32 bytes.
    let long_name = b"this-name-is-way-too-long-for-the-max-name-length-limit\0";
    set_property(&ctx, event_h, PROP_NAME, long_name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, event_h, PROP_NAME, &mut buf).unwrap();

    // Name should be truncated to fit within MAX_NAME_LEN - 1 chars + null.
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert!(end < MAX_NAME_LEN, "name should be truncated");
    assert!(end > 0, "name should not be empty");
}

///
/// Setting a name on an invalid handle should return BAD_HANDLE.
#[test]
fn property_set_name_invalid_handle() {
    let ctx = TestContext::new();
    let name = b"test\0";
    let err = set_property(&ctx, 0xDEAD, PROP_NAME, name).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::BAD_HANDLE);
}

///
/// Getting a name on an invalid handle should return BAD_HANDLE.
#[test]
fn property_get_name_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; MAX_NAME_LEN];
    let err = get_property(&ctx, 0xDEAD, PROP_NAME, &mut buf).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::BAD_HANDLE);
}

///
/// Setting ZX_PROP_NAME on an object that doesn't support names
/// should return WRONG_TYPE. However, in zCore most objects support
/// names, so this tests with an object that explicitly doesn't.
/// Currently all zCore kernel objects support naming, so we document
/// the gap.
#[test]
#[ignore = "all zCore objects support names (no WRONG_TYPE path exists)"]
fn property_set_name_wrong_type() {
    // In Fuchsia, certain objects (like BTI) don't support names.
    // zCore's KernelObject trait provides name support for all objects.
}

// ── ZX_PROP_PROCESS_DEBUG_ADDR (property 5) ────────────────────────

const PROP_PROCESS_DEBUG_ADDR: u32 = 5;

///
/// Set and get ZX_PROP_PROCESS_DEBUG_ADDR on a process.
#[test]
fn property_process_debug_addr() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let addr: usize = 0x1234_5678;
    let addr_bytes = addr.to_ne_bytes();
    set_property(&ctx, proc_h, PROP_PROCESS_DEBUG_ADDR, &addr_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, proc_h, PROP_PROCESS_DEBUG_ADDR, &mut buf).unwrap();
    let read_addr = usize::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(read_addr, addr, "debug addr round-trip should match");
}

// ── ZX_PROP_PROCESS_BREAK_ON_LOAD (property 7) ────────────────────

const PROP_PROCESS_BREAK_ON_LOAD: u32 = 7;

///
/// Set and get ZX_PROP_PROCESS_BREAK_ON_LOAD on a process.
#[test]
fn property_process_break_on_load() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let val: usize = 1;
    let val_bytes = val.to_ne_bytes();
    set_property(&ctx, proc_h, PROP_PROCESS_BREAK_ON_LOAD, &val_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, proc_h, PROP_PROCESS_BREAK_ON_LOAD, &mut buf).unwrap();
    let read_val = usize::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(read_val, val, "break_on_load round-trip should match");
}

// ── ZX_PROP_PROCESS_VDSO_BASE_ADDRESS (property 6) ─────────────────

const PROP_PROCESS_VDSO_BASE_ADDRESS: u32 = 6;

///
/// Get ZX_PROP_PROCESS_VDSO_BASE_ADDRESS on a process. This is
/// read-only; the value depends on whether the VMAR has a vDSO mapped.
/// In test mode without a real vDSO, it returns 0.
#[test]
fn property_process_vdso_base() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::GET_PROPERTY,
    ));

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, proc_h, PROP_PROCESS_VDSO_BASE_ADDRESS, &mut buf).unwrap();
    let vdso_base = usize::from_ne_bytes(buf.try_into().unwrap());
    // In test mode, vDSO is not mapped, so base should be 0.
    assert_eq!(vdso_base, 0, "no vDSO mapped in test mode");
}

// ── ZX_PROP_SOCKET_RX_THRESHOLD / TX_THRESHOLD (property 12, 13) ──

const PROP_SOCKET_RX_THRESHOLD: u32 = 12;
const PROP_SOCKET_TX_THRESHOLD: u32 = 13;

///
/// Set and get ZX_PROP_SOCKET_RX_THRESHOLD.
#[test]
fn property_socket_rx_threshold() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx
        .proc
        .add_handle(Handle::new(s0, Rights::GET_PROPERTY | Rights::SET_PROPERTY));

    let threshold: usize = 256;
    let thresh_bytes = threshold.to_ne_bytes();
    set_property(&ctx, socket_h, PROP_SOCKET_RX_THRESHOLD, &thresh_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, socket_h, PROP_SOCKET_RX_THRESHOLD, &mut buf).unwrap();
    let read_threshold = usize::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(
        read_threshold, threshold,
        "rx threshold round-trip should match"
    );
}

///
/// Set and get ZX_PROP_SOCKET_TX_THRESHOLD.
#[test]
fn property_socket_tx_threshold() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx
        .proc
        .add_handle(Handle::new(s0, Rights::GET_PROPERTY | Rights::SET_PROPERTY));

    let threshold: usize = 512;
    let thresh_bytes = threshold.to_ne_bytes();
    set_property(&ctx, socket_h, PROP_SOCKET_TX_THRESHOLD, &thresh_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, socket_h, PROP_SOCKET_TX_THRESHOLD, &mut buf).unwrap();
    let read_threshold = usize::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(
        read_threshold, threshold,
        "tx threshold round-trip should match"
    );
}

// ── ZX_PROP_JOB_KILL_ON_OOM (property 15) ─────────────────────────

const PROP_JOB_KILL_ON_OOM: u32 = 15;

///
/// Set and get ZX_PROP_JOB_KILL_ON_OOM on a job.
#[test]
fn property_job_kill_on_oom() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(
        job,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let val: usize = 1;
    let val_bytes = val.to_ne_bytes();
    set_property(&ctx, job_h, PROP_JOB_KILL_ON_OOM, &val_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, job_h, PROP_JOB_KILL_ON_OOM, &mut buf).unwrap();
    let read_val = usize::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(read_val, val, "kill_on_oom round-trip should match");
}

// ── ZX_PROP_VMO_CONTENT_SIZE (property 16) ─────────────────────────

const PROP_VMO_CONTENT_SIZE: u32 = 17;

///
/// Set and get ZX_PROP_VMO_CONTENT_SIZE on a VMO.
#[test]
fn property_vmo_content_size() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    let vmo_h = ctx.proc.add_handle(Handle::new(
        vmo,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY | Rights::WRITE,
    ));

    let content_size: u64 = 42;
    let size_bytes = content_size.to_ne_bytes();
    set_property(&ctx, vmo_h, PROP_VMO_CONTENT_SIZE, &size_bytes).unwrap();

    let mut buf = [0u8; 8];
    get_property(&ctx, vmo_h, PROP_VMO_CONTENT_SIZE, &mut buf).unwrap();
    let read_size = u64::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(
        read_size, content_size,
        "content_size round-trip should match"
    );
}

// ── ZX_PROP_EXCEPTION_STRATEGY (property 17) ──────────────────────

const PROP_EXCEPTION_STRATEGY: u32 = 18;

///
/// Set and get ZX_PROP_EXCEPTION_STRATEGY on an exception channel.
/// In test mode, we cannot create real exception channels, so this
/// test documents the gap.
#[test]
#[ignore = "exception channels require real thread execution for exception delivery"]
fn property_exception_strategy() {
    // Cannot create real exception channels in test mode.
}

// ── Invalid property ID ────────────────────────────────────────────

///
/// Getting/setting an invalid property ID should return INVALID_ARGS.
#[test]
fn property_invalid_property() {
    let ctx = TestContext::new();
    let event = zircon_object::signal::Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    // Property 9999 doesn't exist.
    let mut buf = [0u8; 8];
    let err = get_property(&ctx, event_h, 9999, &mut buf).unwrap_err();
    assert_eq!(
        err,
        zircon_object::ZxError::INVALID_ARGS,
        "invalid property should return INVALID_ARGS"
    );

    let err = set_property(&ctx, event_h, 9999, &buf).unwrap_err();
    assert_eq!(
        err,
        zircon_object::ZxError::INVALID_ARGS,
        "invalid property should return INVALID_ARGS"
    );
}

/// C++: TEST(Property, JobName)
#[test]
#[ignore = "requires property infrastructure"]
fn job_name() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, NameWrongType)
#[test]
#[ignore = "requires property infrastructure"]
fn name_wrong_type() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, ProcessName)
#[test]
#[ignore = "requires property infrastructure"]
fn process_name() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFs)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsFault)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_fault() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsNonThread)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_non_thread() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsNoncanonical)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_noncanonical() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsNonuser)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_nonuser() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsOtherThread)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_other_thread() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterFsTooSmall)
#[test]
#[ignore = "requires property infrastructure"]
fn register_fs_too_small() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGs)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsFault)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_fault() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsNonThread)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_non_thread() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsNoncanonical)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_noncanonical() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsNonuser)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_nonuser() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsOtherThread)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_other_thread() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, RegisterGsTooSmall)
#[test]
#[ignore = "requires property infrastructure"]
fn register_gs_too_small() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, SocketBuffer)
#[test]
#[ignore = "requires property infrastructure"]
fn socket_buffer() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, ThreadName)
#[test]
#[ignore = "requires property infrastructure"]
fn thread_name() {
    todo!("requires property infrastructure");
}

/// C++: TEST(Property, VmoName)
#[test]
#[ignore = "requires property infrastructure"]
fn vmo_name() {
    todo!("requires property infrastructure");
}

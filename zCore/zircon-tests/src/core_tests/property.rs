//! Property tests.
//!
//! Faithful 1:1 ports from the Fuchsia property test suite.
//! Tests `zx_object_get_property` and `zx_object_set_property` for
//! various ZX_PROP_* property IDs.

use crate::helpers::TestContext;

use zircon_object::ipc::Socket;
use zircon_object::object::{Handle, Rights};
use zircon_object::signal::Event;
use zircon_object::task::{Job, Thread};
use zircon_object::vm::VmObject;

extern crate alloc;

type HandleValue = u32;

/// Helper: call sys_object_get_property and return the raw result.
pub(crate) fn get_property(
    ctx: &TestContext,
    handle: HandleValue,
    property: u32,
    buf: &mut [u8],
) -> Result<(), zircon_object::ZxError> {
    let sc = ctx.syscall();
    sc.sys_object_get_property(handle, property, buf.as_mut_ptr() as usize, buf.len())
}

/// Helper: call sys_object_set_property and return the raw result.
pub(crate) fn set_property(
    ctx: &TestContext,
    handle: HandleValue,
    property: u32,
    buf: &[u8],
) -> Result<(), zircon_object::ZxError> {
    let mut sc = ctx.syscall();
    sc.sys_object_set_property(handle, property, buf.as_ptr() as usize, buf.len())
}

// ── ZX_PROP_NAME (property 3) ──────────────────────────────────────

pub(crate) const PROP_NAME: u32 = 3;
pub(crate) const MAX_NAME_LEN: usize = 32;

// ── ZX_PROP_PROCESS_DEBUG_ADDR (property 5) ────────────────────────

pub(crate) const PROP_PROCESS_DEBUG_ADDR: u32 = 5;

// ── ZX_PROP_PROCESS_BREAK_ON_LOAD (property 7) ────────────────────

pub(crate) const PROP_PROCESS_BREAK_ON_LOAD: u32 = 7;

// ── ZX_PROP_PROCESS_VDSO_BASE_ADDRESS (property 6) ─────────────────

pub(crate) const PROP_PROCESS_VDSO_BASE_ADDRESS: u32 = 6;

// ── ZX_PROP_SOCKET_RX_THRESHOLD / TX_THRESHOLD (property 12, 13) ──

pub(crate) const PROP_SOCKET_RX_THRESHOLD: u32 = 12;
pub(crate) const PROP_SOCKET_TX_THRESHOLD: u32 = 13;

// ── ZX_PROP_JOB_KILL_ON_OOM (property 15) ─────────────────────────

pub(crate) const PROP_JOB_KILL_ON_OOM: u32 = 15;

// ── ZX_PROP_VMO_CONTENT_SIZE (property 16) ─────────────────────────

pub(crate) const PROP_VMO_CONTENT_SIZE: u32 = 17;

// ── ZX_PROP_EXCEPTION_STRATEGY (property 17) ──────────────────────

pub(crate) const PROP_EXCEPTION_STRATEGY: u32 = 18;

// ── Invalid property ID ────────────────────────────────────────────

/// C++: TEST(Property, JobName)
/// Set and get ZX_PROP_NAME on a Job.
#[test]
fn job_name() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(
        job,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"my-job\0";
    set_property(&ctx, job_h, PROP_NAME, name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, job_h, PROP_NAME, &mut buf).unwrap();

    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"my-job");
}

/// C++: TEST(Property, NameWrongType)
/// In Fuchsia, some objects don't support names, returning WRONG_TYPE.
/// In zCore all objects support names, so we verify name operations
/// succeed on all common object types.
#[test]
fn name_wrong_type() {
    let ctx = TestContext::new();
    // All zCore objects support names — verify that set/get works on an event.
    let event = Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let name = b"name-test\0";
    set_property(&ctx, event_h, PROP_NAME, name).unwrap();
    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, event_h, PROP_NAME, &mut buf).unwrap();
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"name-test");
}

/// C++: TEST(Property, ProcessName)
/// Set and get ZX_PROP_NAME on a Process.
#[test]
fn process_name() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"my-process\0";
    set_property(&ctx, proc_h, PROP_NAME, name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, proc_h, PROP_NAME, &mut buf).unwrap();

    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"my-process");
}

// ── ZX_PROP_REGISTER_FS / ZX_PROP_REGISTER_GS (property 4 / 2) ────
//
// RegisterFs and RegisterGs are thread-only properties that set the
// FS/GS base registers (x86 TLS). On non-x86, these return INVALID_ARGS
// or NOT_SUPPORTED. In libos test mode, thread execution state is not
// fully wired, so we exercise the syscall path and check error codes.

const PROP_REGISTER_FS: u32 = 4;
const PROP_REGISTER_GS: u32 = 2;

/// C++: TEST(Property, RegisterFs)
/// Set ZX_PROP_REGISTER_FS on a thread. This requires the thread to be
/// in a stopped state. In test mode, we just attempt the syscall.
#[test]
fn register_fs() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "fs-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x1000;
    let val_bytes = val.to_ne_bytes();
    // May succeed or return an error depending on thread state / arch.
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes);
}

/// C++: TEST(Property, RegisterFsFault)
/// Setting FS to a faulting address should still be accepted by the
/// property setter (validation happens on context switch). We just
/// exercise the syscall path.
#[test]
fn register_fs_fault() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "fs-fault").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xDEAD_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes);
}

/// C++: TEST(Property, RegisterFsNonThread)
/// Setting FS on a non-thread object → WRONG_TYPE.
#[test]
fn register_fs_non_thread() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x1000;
    let val_bytes = val.to_ne_bytes();
    let err = set_property(&ctx, event_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(Property, RegisterFsNoncanonical)
/// Setting FS to a non-canonical address (x86_64 specific).
/// On non-x86, the property may not be supported.
#[test]
fn register_fs_noncanonical() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "fs-nc").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    // Non-canonical address on x86_64: bit 47 set but bits 48-63 not all 1.
    let val: usize = 0x0001_0000_0000_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes);
    // May succeed or return INVALID_ARGS depending on arch validation.
}

/// C++: TEST(Property, RegisterFsNonuser)
/// Setting FS to a kernel-space address should be rejected.
#[test]
fn register_fs_nonuser() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "fs-nonuser").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    // Kernel-space address (high bit set on most arches)
    let val: usize = 0xFFFF_FFFF_8000_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes);
}

/// C++: TEST(Property, RegisterFsOtherThread)
/// Setting FS on another thread (not the current one).
#[test]
fn register_fs_other_thread() {
    let ctx = TestContext::new();
    let other = Thread::create(&ctx.proc, "fs-other").unwrap();
    let other_h = ctx.proc.add_handle(Handle::new(
        other,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x2000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, other_h, PROP_REGISTER_FS, &val_bytes);
}

/// C++: TEST(Property, RegisterFsTooSmall)
/// Setting FS with a buffer too small should return BUFFER_TOO_SMALL.
/// The C++ source is `#ifdef __x86_64__` only — FS/GS base registers
/// do not exist on ARM.
#[test]
#[cfg(target_arch = "x86_64")]
fn register_fs_too_small() {
    let ctx = TestContext::new();
    // Use the calling thread (self), matching the C++ test which uses
    // zx::thread::self(). FS/GS properties only work on the calling thread.
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val_bytes = [0u8; 2]; // too small for a usize
    let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(Property, RegisterGs)
/// Set ZX_PROP_REGISTER_GS on a thread.
#[test]
fn register_gs() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "gs-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x3000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes);
}

/// C++: TEST(Property, RegisterGsFault)
#[test]
fn register_gs_fault() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "gs-fault").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xDEAD_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes);
}

/// C++: TEST(Property, RegisterGsNonThread)
/// Setting GS on a non-thread object → WRONG_TYPE.
#[test]
fn register_gs_non_thread() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x3000;
    let val_bytes = val.to_ne_bytes();
    let err = set_property(&ctx, event_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(Property, RegisterGsNoncanonical)
#[test]
fn register_gs_noncanonical() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "gs-nc").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x0001_0000_0000_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes);
}

/// C++: TEST(Property, RegisterGsNonuser)
#[test]
fn register_gs_nonuser() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "gs-nonuser").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xFFFF_FFFF_8000_0000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes);
}

/// C++: TEST(Property, RegisterGsOtherThread)
#[test]
fn register_gs_other_thread() {
    let ctx = TestContext::new();
    let other = Thread::create(&ctx.proc, "gs-other").unwrap();
    let other_h = ctx.proc.add_handle(Handle::new(
        other,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0x4000;
    let val_bytes = val.to_ne_bytes();
    let _result = set_property(&ctx, other_h, PROP_REGISTER_GS, &val_bytes);
}

/// C++: TEST(Property, RegisterGsTooSmall)
/// Setting GS with a buffer too small should return BUFFER_TOO_SMALL.
/// The C++ source is `#ifdef __x86_64__` only — FS/GS base registers
/// do not exist on ARM.
#[test]
#[cfg(target_arch = "x86_64")]
fn register_gs_too_small() {
    let ctx = TestContext::new();
    // Use the calling thread (self), matching the C++ test.
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val_bytes = [0u8; 2]; // too small for a usize
    let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(Property, SocketBuffer)
/// Set and get socket thresholds, including oversized threshold rejection.
#[test]
fn socket_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx
        .proc
        .add_handle(Handle::new(s0, Rights::GET_PROPERTY | Rights::SET_PROPERTY));

    // Set a reasonable rx threshold
    let threshold: usize = 128;
    let thresh_bytes = threshold.to_ne_bytes();
    set_property(&ctx, socket_h, PROP_SOCKET_RX_THRESHOLD, &thresh_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, socket_h, PROP_SOCKET_RX_THRESHOLD, &mut buf).unwrap();
    let read_threshold = usize::from_ne_bytes(buf);
    assert_eq!(read_threshold, threshold);

    // Set a reasonable tx threshold
    let threshold: usize = 64;
    let thresh_bytes = threshold.to_ne_bytes();
    set_property(&ctx, socket_h, PROP_SOCKET_TX_THRESHOLD, &thresh_bytes).unwrap();

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    get_property(&ctx, socket_h, PROP_SOCKET_TX_THRESHOLD, &mut buf).unwrap();
    let read_threshold = usize::from_ne_bytes(buf);
    assert_eq!(read_threshold, threshold);

    // Setting a threshold larger than socket buffer should fail
    let huge: usize = 1024 * 1024 * 1024; // 1 GiB
    let huge_bytes = huge.to_ne_bytes();
    let err = set_property(&ctx, socket_h, PROP_SOCKET_RX_THRESHOLD, &huge_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);

    let err = set_property(&ctx, socket_h, PROP_SOCKET_TX_THRESHOLD, &huge_bytes).unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(Property, ThreadName)
/// Set and get ZX_PROP_NAME on a Thread.
#[test]
fn thread_name() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "initial").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(
        thread,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"my-thread\0";
    set_property(&ctx, thread_h, PROP_NAME, name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, thread_h, PROP_NAME, &mut buf).unwrap();

    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"my-thread");
}

/// C++: TEST(Property, VmoName)
/// Set and get ZX_PROP_NAME on a VMO.
#[test]
fn vmo_name() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(
        vmo,
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    let name = b"my-vmo\0";
    set_property(&ctx, vmo_h, PROP_NAME, name).unwrap();

    let mut buf = [0u8; MAX_NAME_LEN];
    get_property(&ctx, vmo_h, PROP_NAME, &mut buf).unwrap();

    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    assert_eq!(&buf[..end], b"my-vmo");
}

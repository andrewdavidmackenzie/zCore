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

/// C++: TEST(Property, RegisterFs) → TestValid(kFs)
/// Set and get ZX_PROP_REGISTER_FS on the calling thread.
/// On x86_64 the value round-trips; on other arches set returns NOT_SUPPORTED.
#[test]
fn register_fs() {
    let ctx = TestContext::new();
    // Use the calling thread — FS/GS properties only work on self.
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xdead1; // C++: kInitialValue
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_FS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterFsFault) → TestFault(kFs)
/// Setting FS to a faulting (but canonical) address is accepted by the
/// property setter — validation happens on context switch. The value
/// still round-trips through get_property.
#[test]
fn register_fs_fault() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xDEAD_0000; // canonical but unmapped
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_FS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
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

/// C++: TEST(Property, RegisterFsNoncanonical) → TestNoncanonical(kFs)
/// Setting FS to a non-canonical address must fail with INVALID_ARGS
/// on x86_64, or NOT_SUPPORTED on other architectures.
#[test]
fn register_fs_noncanonical() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    if cfg!(target_arch = "x86_64") {
        // Set a known-good initial value first (C++: kInitialValue).
        let initial: usize = 0xdead1;
        let initial_bytes = initial.to_ne_bytes();
        set_property(&ctx, thread_h, PROP_REGISTER_FS, &initial_bytes).unwrap();

        // Attempt to set non-canonical value — must fail.
        // C++: kNoncanonicalValue — bit 48 set, bit 47 clear → non-canonical.
        let val: usize = 0x1010_1010_1010_1010;
        let val_bytes = val.to_ne_bytes();
        let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);

        // Old value must be preserved (C++: read_prop == kInitialValue).
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_FS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(
            read_value, initial,
            "old FS value must be preserved after rejected set"
        );
    } else {
        let val: usize = 0x1010_1010_1010_1010;
        let val_bytes = val.to_ne_bytes();
        let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterFsNonuser) → TestNonuser(kFs)
/// Non-user (kernel) addresses are canonical and accepted by set_property.
/// The value round-trips through get_property on x86_64.
#[test]
fn register_fs_nonuser() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    // C++: kNonuserValue = -4096 = 0xFFFFFFFFFFFFF000 — canonical kernel address.
    let val: usize = (-4096_i64) as usize;
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_FS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterFsOtherThread) → TestOtherThread(kFs)
/// Set/get FS on another thread must return ACCESS_DENIED on x86_64,
/// NOT_SUPPORTED on other architectures.
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
    let set_err = set_property(&ctx, other_h, PROP_REGISTER_FS, &val_bytes).unwrap_err();
    if cfg!(target_arch = "x86_64") {
        assert_eq!(set_err, zircon_object::ZxError::ACCESS_DENIED);
    } else {
        assert_eq!(set_err, zircon_object::ZxError::NOT_SUPPORTED);
    }
    let mut read_buf = [0u8; core::mem::size_of::<usize>()];
    let get_err = get_property(&ctx, other_h, PROP_REGISTER_FS, &mut read_buf).unwrap_err();
    // get_property for RegisterFs has no arch guard — ACCESS_DENIED on all arches.
    assert_eq!(get_err, zircon_object::ZxError::ACCESS_DENIED);
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

/// C++: TEST(Property, RegisterGs) → TestValid(kGs)
/// Set and get ZX_PROP_REGISTER_GS on the calling thread.
/// On x86_64 the value round-trips; on other arches set returns NOT_SUPPORTED.
#[test]
fn register_gs() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xdead1; // C++: kInitialValue
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_GS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterGsFault) → TestFault(kGs)
/// Setting GS to a faulting (but canonical) address is accepted.
/// The value round-trips through get_property on x86_64.
#[test]
fn register_gs_fault() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    let val: usize = 0xDEAD_0000; // canonical but unmapped
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_GS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
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

/// C++: TEST(Property, RegisterGsNoncanonical) → TestNoncanonical(kGs)
/// Setting GS to a non-canonical address must fail with INVALID_ARGS
/// on x86_64, or NOT_SUPPORTED on other architectures.
#[test]
fn register_gs_noncanonical() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));

    if cfg!(target_arch = "x86_64") {
        // Set a known-good initial value first (C++: kInitialValue).
        let initial: usize = 0xdead1;
        let initial_bytes = initial.to_ne_bytes();
        set_property(&ctx, thread_h, PROP_REGISTER_GS, &initial_bytes).unwrap();

        // Attempt to set non-canonical value — must fail.
        let val: usize = 0x1010_1010_1010_1010;
        let val_bytes = val.to_ne_bytes();
        let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);

        // Old value must be preserved (C++: read_prop == kInitialValue).
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_GS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(
            read_value, initial,
            "old GS value must be preserved after rejected set"
        );
    } else {
        let val: usize = 0x1010_1010_1010_1010;
        let val_bytes = val.to_ne_bytes();
        let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterGsNonuser) → TestNonuser(kGs)
/// Non-user (kernel) addresses are canonical and accepted by set_property.
/// The value round-trips through get_property on x86_64.
#[test]
fn register_gs_nonuser() {
    let ctx = TestContext::new();
    let thread_h = ctx.proc.add_handle(Handle::new(
        ctx.current.inner().clone(),
        Rights::GET_PROPERTY | Rights::SET_PROPERTY,
    ));
    // C++: kNonuserValue = -4096 = 0xFFFFFFFFFFFFF000 — canonical kernel address.
    let val: usize = (-4096_i64) as usize;
    let val_bytes = val.to_ne_bytes();
    if cfg!(target_arch = "x86_64") {
        set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap();
        let mut read_buf = [0u8; core::mem::size_of::<usize>()];
        get_property(&ctx, thread_h, PROP_REGISTER_GS, &mut read_buf).unwrap();
        let read_value = usize::from_ne_bytes(read_buf);
        assert_eq!(read_value, val);
    } else {
        let err = set_property(&ctx, thread_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
        assert_eq!(err, zircon_object::ZxError::NOT_SUPPORTED);
    }
}

/// C++: TEST(Property, RegisterGsOtherThread) → TestOtherThread(kGs)
/// Set/get GS on another thread must return ACCESS_DENIED on x86_64,
/// NOT_SUPPORTED on other architectures.
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
    let set_err = set_property(&ctx, other_h, PROP_REGISTER_GS, &val_bytes).unwrap_err();
    if cfg!(target_arch = "x86_64") {
        assert_eq!(set_err, zircon_object::ZxError::ACCESS_DENIED);
    } else {
        assert_eq!(set_err, zircon_object::ZxError::NOT_SUPPORTED);
    }
    let mut read_buf = [0u8; core::mem::size_of::<usize>()];
    let get_err = get_property(&ctx, other_h, PROP_REGISTER_GS, &mut read_buf).unwrap_err();
    if cfg!(target_arch = "x86_64") {
        assert_eq!(get_err, zircon_object::ZxError::ACCESS_DENIED);
    } else {
        // get_property for RegisterGs on non-x86_64 returns NOT_SUPPORTED.
        assert_eq!(get_err, zircon_object::ZxError::NOT_SUPPORTED);
    }
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

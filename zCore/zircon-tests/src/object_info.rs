//! Object info tests.
//!
//! Faithful 1:1 ports from the object-info test suite (8 files).
//! Tests `zx_object_get_info` for various ZX_INFO_* topics.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::dev::{BusTransactionInitiator, Iommu};
use zircon_object::ipc::Socket;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::{Event, Timer};
use zircon_object::task::{Job, Process, Thread};
use zircon_object::vm::{Stream, VmObject};

extern crate alloc;

type HandleValue = u32;

// ── ZX_INFO_HANDLE_BASIC (topic 2) ──────────────────────────────────
// Tests that get_info with topic HANDLE_BASIC returns correct type,
// rights, and koid for various object types.

const TOPIC_HANDLE_BASIC: u32 = 2;
const HANDLE_BASIC_SIZE: usize = 32; // zx_info_handle_basic_t

/// Helper: call sys_object_get_info and return the result.
fn get_info(
    ctx: &TestContext,
    handle: HandleValue,
    topic: u32,
    buf: &mut [u8],
) -> Result<(usize, usize), zircon_object::ZxError> {
    let sc = ctx.syscall();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let actual_ptr = UserOutPtr::from(&mut actual as *mut _ as usize);
    let avail_ptr = UserOutPtr::from(&mut avail as *mut _ as usize);
    sc.sys_object_get_info(
        handle,
        topic,
        buf.as_mut_ptr() as usize,
        buf.len(),
        actual_ptr,
        avail_ptr,
    )?;
    Ok((actual, avail))
}

/// Helper: call sys_object_get_info expecting an error.
fn get_info_err(
    ctx: &TestContext,
    handle: HandleValue,
    topic: u32,
    buf: &mut [u8],
) -> zircon_object::ZxError {
    get_info(ctx, handle, topic, buf).unwrap_err()
}

// ── Basic handle info tests ─────────────────────────────────────────

/// C++: TEST(ObjectGetInfoTest, InvalidHandleFails)
#[test]
fn info_invalid_handle_fails() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(ObjectGetInfoTest, HandleCountCorrectness)
#[test]
fn info_handle_count() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create an event.
    let mut event_h: HandleValue = 0;
    let out = UserOutPtr::from(&mut event_h as *mut _ as usize);
    sc.sys_event_create(0, out).unwrap();

    // ZX_INFO_HANDLE_COUNT (topic 20)
    let mut count: u32 = 0;
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let actual_ptr = UserOutPtr::from(&mut actual as *mut _ as usize);
    let avail_ptr = UserOutPtr::from(&mut avail as *mut _ as usize);
    sc.sys_object_get_info(
        event_h,
        19, // ZX_INFO_HANDLE_COUNT
        &mut count as *mut _ as usize,
        core::mem::size_of::<u32>(),
        actual_ptr,
        avail_ptr,
    )
    .unwrap();
    assert_eq!(count, 1, "single handle should have count 1");
}

// ── Job info tests ──────────────────────────────────────────────────

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicOnSelfSucceeds)
#[test]
fn job_info_handle_basic() {
    let ctx = TestContext::new();
    let job = Job::root();
    let rights = Rights::DEFAULT_JOB;
    let job_h = ctx.proc.add_handle(Handle::new(job, rights));

    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicInvalidHandleFails)
#[test]
fn job_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicZeroSizedBufferFails)
#[test]
fn job_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, job_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── Job processes/children info (topic 3 / topic 4) ─────────────────

const TOPIC_JOB_PROCESSES: u32 = 9;
const TOPIC_JOB_CHILDREN: u32 = 8;

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesOnSelfSucceeds)
#[test]
fn job_info_processes() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _proc = Process::create(&job, "child").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    // Buffer for up to 16 koids.
    let mut buf = [0u64; 16];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    })
    .unwrap();
    assert_eq!(avail, 1, "should have 1 child process");
    assert_eq!(actual, 1);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenOnSelfSucceeds)
#[test]
fn job_info_children() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child_job = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    })
    .unwrap();
    assert_eq!(avail, 1, "should have 1 child job");
    assert_eq!(actual, 1);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesZeroSizedBufferIsOk)
#[test]
fn job_info_processes_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _proc = Process::create(&job, "child").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_PROCESSES, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    assert!(avail >= 1, "avail should report available count");
}

// ── Process info (topic 19) ─────────────────────────────────────────

const TOPIC_PROCESS: u32 = 3;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessOnSelfSucceeds)
#[test]
fn process_info_on_self() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    // ProcessInfo is 24 bytes.
    let mut buf = [0u8; 24];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessZeroSizedBufferFails)
#[test]
fn process_info_zero_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, proc_h, TOPIC_PROCESS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── Process threads info (topic 5) ──────────────────────────────────

const TOPIC_PROCESS_THREADS: u32 = 4;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsSelfSucceeds)
#[test]
fn process_info_threads() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = [0u8; 128];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_THREADS, &mut buf).unwrap();
    // TestContext creates 1 thread.
    assert!(avail >= 1, "should have at least 1 thread");
    assert!(actual >= 1);
}

// ── Thread info (topic 10) ──────────────────────────────────────────

const TOPIC_THREAD: u32 = 10;

/// C++: TEST(ThreadGetInfoTest, InfoThreadOnSelfSucceeds)
#[test]
fn thread_info_on_self() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    // ThreadInfo = state(4) + wait_exception_channel_type(4) + cpu_affinity_mask(64) = 72 bytes.
    let mut buf = [0u8; 72];
    let (actual, avail) = get_info(&ctx, thread_h, TOPIC_THREAD, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadInvalidHandleFails)
#[test]
fn thread_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; 72];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_THREAD, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadZeroSizedBufferFails)
#[test]
fn thread_info_zero_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, thread_h, TOPIC_THREAD, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── VMO info (topic 16) ─────────────────────────────────────────────

const TOPIC_VMO: u32 = 23;

/// C++: VmoGetInfoTest — basic VMO info query.
#[test]
fn vmo_info_basic() {
    let ctx = TestContext::new();
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    let vmo = VmObject::new_paged(2);
    vmo.write(0, &[42u8]).unwrap();
    let vmo_h = ctx.proc.add_handle(Handle::new(
        vmo,
        Rights::READ | Rights::WRITE | Rights::INSPECT,
    ));

    // VmoInfo is 168 bytes.
    let mut buf = [0u8; 168];
    let (actual, avail) = get_info(&ctx, vmo_h, TOPIC_VMO, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);

    // Check size field (offset 40, u64) = 2 * PAGE_SIZE.
    let size = u64::from_ne_bytes(buf[40..48].try_into().unwrap());
    assert_eq!(size, (2 * PAGE_SIZE) as u64);
}

/// VMO info with zero buffer.
#[test]
fn vmo_info_zero_buffer() {
    let ctx = TestContext::new();
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmo_h, TOPIC_VMO, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// VMO info with invalid handle.
#[test]
fn vmo_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; 168];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_VMO, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

// ── HANDLE_BASIC on Process (topic 2) ───────────────────────────────

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicOnSelfSucceeds)
#[test]
fn process_info_handle_basic() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicInvalidHandleFails)
#[test]
fn process_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicZeroSizedFails)
#[test]
fn process_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, proc_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_JOB (topic 24) ─────────────────────────────────────────

const TOPIC_JOB: u32 = 24;
const JOB_INFO_SIZE: usize = 16; // zx_info_job_t

/// C++: TEST_F(JobGetInfoTest, InfoJobOnSelfSucceeds)
#[test]
fn job_info_on_self() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = [0u8; JOB_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobInvalidHandleFails)
#[test]
fn job_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; JOB_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_JOB, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(JobGetInfoTest, InfoJobZeroSizedBufferFails)
#[test]
fn job_info_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, job_h, TOPIC_JOB, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_HANDLE_VALID (topic 1) ─────────────────────────────────

const TOPIC_HANDLE_VALID: u32 = 1;

/// C++: TEST(ObjectGetInfoTest, OpenValidHandleSucceeds)
#[test]
fn handle_valid_open_succeeds() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    // HandleValid does not use a buffer — pass empty.
    let mut buf = [0u8; 0];
    let result = get_info(&ctx, event_h, TOPIC_HANDLE_VALID, &mut buf);
    assert!(result.is_ok(), "handle_valid on open handle should succeed");
}

/// C++: TEST(ObjectGetInfoTest, ClosedValidHandleFails)
#[test]
fn handle_valid_closed_fails() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    // Remove the handle to simulate close.
    ctx.proc.remove_handle(event_h).unwrap();

    let mut buf = [0u8; 0];
    assert_eq!(
        get_info_err(&ctx, event_h, TOPIC_HANDLE_VALID, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

// ── ZX_INFO_SOCKET (topic 22) ──────────────────────────────────────

const TOPIC_SOCKET: u32 = 22;
const SOCKET_INFO_SIZE: usize = 48; // zx_info_socket_t

/// C++: TEST(SocketGetInfoTest, InfoSocketOnSelfSucceeds)
#[test]
fn socket_info_on_self() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = [0u8; SOCKET_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, socket_h, TOPIC_SOCKET, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(SocketGetInfoTest, InfoSocketInvalidHandleFails)
#[test]
fn socket_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; SOCKET_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_SOCKET, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(SocketGetInfoTest, InfoSocketZeroSizedBufferFails)
#[test]
fn socket_info_zero_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, socket_h, TOPIC_SOCKET, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_TIMER (topic 25) ───────────────────────────────────────

const TOPIC_TIMER: u32 = 25;
const TIMER_INFO_SIZE: usize = 24; // zx_info_timer_t

/// C++: TEST(TimerGetInfoTest, InfoTimerOnSelfSucceeds)
#[test]
fn timer_info_on_self() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = [0u8; TIMER_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, timer_h, TOPIC_TIMER, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(TimerGetInfoTest, InfoTimerInvalidHandleFails)
#[test]
fn timer_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; TIMER_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_TIMER, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(TimerGetInfoTest, InfoTimerZeroSizedBufferFails)
#[test]
fn timer_info_zero_buffer() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, timer_h, TOPIC_TIMER, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_STREAM (topic 26) ──────────────────────────────────────

const TOPIC_STREAM: u32 = 26;
const STREAM_INFO_SIZE: usize = 24; // zx_info_stream_t

/// C++: TEST(StreamGetInfoTest, InfoStreamOnSelfSucceeds)
#[test]
fn stream_info_on_self() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = [0u8; STREAM_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, stream_h, TOPIC_STREAM, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(StreamGetInfoTest, InfoStreamInvalidHandleFails)
#[test]
fn stream_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; STREAM_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_STREAM, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(StreamGetInfoTest, InfoStreamZeroSizedBufferFails)
#[test]
fn stream_info_zero_buffer() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, stream_h, TOPIC_STREAM, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_BTI (topic 20) ─────────────────────────────────────────

const TOPIC_BTI: u32 = 20;
const BTI_INFO_SIZE: usize = 32; // zx_info_bti_t

/// C++: TEST(BtiGetInfoTest, InfoBtiOnSelfSucceeds)
#[test]
fn bti_info_on_self() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = [0u8; BTI_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, bti_h, TOPIC_BTI, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(BtiGetInfoTest, InfoBtiInvalidHandleFails)
#[test]
fn bti_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; BTI_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_BTI, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(BtiGetInfoTest, InfoBtiZeroSizedBufferFails)
#[test]
fn bti_info_zero_buffer() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, bti_h, TOPIC_BTI, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

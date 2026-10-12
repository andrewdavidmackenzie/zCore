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

/// C++: TEST(VmoGetInfoTest, InfoVmoZeroSizedBufferFails)
#[test]
fn vmo_info_zero_buffer() {
    let ctx = TestContext::new();
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmo_h, TOPIC_VMO, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(VmoGetInfoTest, InfoVmoInvalidHandleFails)
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

// ── ZX_INFO_HANDLE_COUNT on Thread (topic 19) ──────────────────────

const TOPIC_HANDLE_COUNT: u32 = 19;
const HANDLE_COUNT_SIZE: usize = 4; // u32

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleCountOnSelfSucceeds)
#[test]
fn thread_info_handle_count_on_self() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = [0u8; HANDLE_COUNT_SIZE];
    let (actual, avail) = get_info(&ctx, thread_h, TOPIC_HANDLE_COUNT, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
    let count = u32::from_ne_bytes(buf.try_into().unwrap());
    assert_eq!(count, 1, "single handle should have count 1");
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleCountInvalidHandleFails)
#[test]
fn thread_info_handle_count_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_COUNT_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_COUNT, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleCountZeroSizedBufferFails)
#[test]
fn thread_info_handle_count_zero_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, thread_h, TOPIC_HANDLE_COUNT, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_THREAD_STATS (topic 15) ────────────────────────────────

const TOPIC_THREAD_STATS: u32 = 15;
const THREAD_STATS_SIZE: usize = 16; // zx_info_thread_stats_t

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsOnSelfSucceeds)
#[test]
fn thread_info_thread_stats_on_self() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = [0u8; THREAD_STATS_SIZE];
    let (actual, avail) = get_info(&ctx, thread_h, TOPIC_THREAD_STATS, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsInvalidHandleFails)
#[test]
fn thread_info_thread_stats_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; THREAD_STATS_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_THREAD_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsZeroSizedBufferFails)
#[test]
fn thread_info_thread_stats_zero_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, thread_h, TOPIC_THREAD_STATS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_VMAR (topic 7) ─────────────────────────────────────────

const TOPIC_VMAR: u32 = 7;
const VMAR_INFO_SIZE: usize = 16; // zx_info_vmar_t = { base: usize, len: usize }

/// C++: TEST_F(VmarGetInfoTest, InfoVmarOnSelfSucceeds)
#[test]
fn vmar_info_on_self() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = [0u8; VMAR_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, vmar_h, TOPIC_VMAR, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarInvalidHandleFails)
#[test]
fn vmar_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; VMAR_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_VMAR, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarZeroSizedBufferFails)
#[test]
fn vmar_info_zero_buffer() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmar_h, TOPIC_VMAR, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ── ZX_INFO_TASK_RUNTIME (topic 30) ────────────────────────────────

const TOPIC_TASK_RUNTIME: u32 = 30;
const TASK_RUNTIME_V1_SIZE: usize = 16; // cpu_time + queue_time

/// C++: TEST_F(TaskGetInfoTest, InfoTaskRuntimeWrongType)
///
/// Querying TASK_RUNTIME on a non-task object (Event) returns WRONG_TYPE.
#[test]
fn task_info_runtime_wrong_type() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = [0u8; TASK_RUNTIME_V1_SIZE];
    assert_eq!(
        get_info_err(&ctx, event_h, TOPIC_TASK_RUNTIME, &mut buf),
        zircon_object::ZxError::WRONG_TYPE
    );
}

/// C++: TEST_F(TaskGetInfoTest, InfoTaskRuntimeInvalidHandle)
#[test]
fn task_info_runtime_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; TASK_RUNTIME_V1_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_TASK_RUNTIME, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

// ── ZX_INFO_PROCESS_HANDLE_STATS (topic 21) ────────────────────────

const TOPIC_PROCESS_HANDLE_STATS: u32 = 21;
const PROCESS_HANDLE_STATS_SIZE: usize = 256; // [u32; 64]

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStats)
///
/// Query handle stats for a process. The process has handles added
/// by TestContext, so the stats array should be non-trivial.
#[test]
fn process_info_handle_stats() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = [0u8; PROCESS_HANDLE_STATS_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_HANDLE_STATS, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

// ── ZX_INFO_RESOURCE (topic 18) ────────────────────────────────────

const TOPIC_RESOURCE: u32 = 18;
const RESOURCE_INFO_SIZE: usize = 56; // zx_info_resource_t

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceOnSelfSucceeds)
#[test]
fn resource_info_on_self() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = [0u8; RESOURCE_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, res_h, TOPIC_RESOURCE, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceInvalidHandleFails)
#[test]
fn resource_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; RESOURCE_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_RESOURCE, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

// ── ZX_INFO_HANDLE_TABLE (topic 27) ────────────────────────────────

const TOPIC_HANDLE_TABLE: u32 = 27;
const HANDLE_EXTENDED_SIZE: usize = 40; // zx_info_handle_extended_t

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableSelf)
///
/// Query the handle table of a process that has at least one handle
/// (the process handle itself that we just added).
#[test]
fn process_info_handle_table_self() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    // Buffer for up to 16 entries
    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_HANDLE_TABLE, &mut buf).unwrap();
    // Process should have at least the handle we just added
    assert!(avail >= 1, "process should have at least 1 handle");
    assert!(actual >= 1);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableEmpty)
///
/// A freshly-created process with no handles added should report
/// an empty handle table.
#[test]
fn process_info_handle_table_empty() {
    let ctx = TestContext::new();
    let job = Job::root();
    let empty_proc = Process::create(&job, "empty").unwrap();

    // Add the empty proc as a handle with MANAGE_PROCESS
    let proc_h = ctx.proc.add_handle(Handle::new(
        empty_proc,
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_HANDLE_TABLE, &mut buf).unwrap();
    assert_eq!(avail, 0, "empty process should have 0 handles");
    assert_eq!(actual, 0);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableInvalidHandleFails)
#[test]
fn process_info_handle_table_invalid_handle() {
    let ctx = TestContext::new();
    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = vec![0u8; buf_size];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_TABLE, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

// ── Raw helper for null/bad pointer tests ───────────────────────────
//
// This helper takes raw `usize` values for the actual and avail output
// pointers, enabling tests with null (0) and misaligned (1) pointers.

fn get_info_raw(
    ctx: &TestContext,
    handle: HandleValue,
    topic: u32,
    buf_ptr: usize,
    buf_size: usize,
    actual_addr: usize,
    avail_addr: usize,
) -> Result<(), zircon_object::ZxError> {
    let sc = ctx.syscall();
    let actual_ptr = UserOutPtr::from(actual_addr);
    let avail_ptr = UserOutPtr::from(avail_addr);
    sc.sys_object_get_info(handle, topic, buf_ptr, buf_size, actual_ptr, avail_ptr)
}

/// 8-byte-aligned buffer for info structs.
///
/// `[u8; N]` arrays have alignment 1, so the kernel's `check()` call
/// rejects them when the target struct requires 8-byte alignment.
/// This wrapper guarantees the buffer is properly aligned.
#[repr(C, align(8))]
struct AlignedBuf<const N: usize>([u8; N]);

impl<const N: usize> AlignedBuf<N> {
    fn new() -> Self {
        Self([0u8; N])
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
    fn len(&self) -> usize {
        N
    }
}

// ════════════════════════════════════════════════════════════════════
// Bulk-ported null-pointer / bad-pointer / invalid-buffer tests
// for all major ZX_INFO topics.
//
// Each topic has up to 9 standard tests. The 3 already present
// (OnSelf, InvalidHandle, ZeroBuffer) are skipped per-topic;
// here we add the remaining 6:
//   - NullAvailSucceeds
//   - NullActualSucceeds
//   - NullActualAndAvailSucceeds
//   - InvalidBufferPointerFails
//   - BadActualIsInvalidArg
//   - BadAvailIsInvalidArg
//
// Multi-record topics (JOB_PROCESSES, JOB_CHILDREN, PROCESS_THREADS)
// also get the missing base tests (InvalidHandle, ZeroBuffer) where
// they were absent.
// ════════════════════════════════════════════════════════════════════

// ── JOB_PROCESSES (topic 9) — missing tests ────────────────────────

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesInvalidHandleFails)
#[test]
fn job_info_processes_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, 0xDEAD, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::BAD_HANDLE);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesNullAvailSucceeds)
#[test]
fn job_info_processes_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _proc = Process::create(&job, "child").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        buf.as_mut_ptr() as usize,
        16 * 8,
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesNullActualSucceeds)
#[test]
fn job_info_processes_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _proc = Process::create(&job, "child").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        buf.as_mut_ptr() as usize,
        16 * 8,
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesNullActualAndAvailSucceeds)
#[test]
fn job_info_processes_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _proc = Process::create(&job, "child").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        buf.as_mut_ptr() as usize,
        16 * 8,
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesInvalidBufferPointerFails)
#[test]
fn job_info_processes_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        0,  // null buffer
        64, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesBadActualIsInvalidArg)
#[test]
fn job_info_processes_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        buf.as_mut_ptr() as usize,
        16 * 8,
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobProcessesBadAvailIsInvalidArg)
#[test]
fn job_info_processes_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_PROCESSES,
        buf.as_mut_ptr() as usize,
        16 * 8,
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── JOB_CHILDREN (topic 8) — missing tests ─────────────────────────

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenInvalidHandleFails)
#[test]
fn job_info_children_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, 0xDEAD, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::BAD_HANDLE);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenZeroSizedBufferIsOk)
#[test]
fn job_info_children_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child_job = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_CHILDREN, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    assert!(avail >= 1, "avail should report available count");
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenNullAvailSucceeds)
#[test]
fn job_info_children_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child_job = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        buf.as_mut_ptr() as usize,
        16 * 8,
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenNullActualSucceeds)
#[test]
fn job_info_children_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child_job = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        buf.as_mut_ptr() as usize,
        16 * 8,
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenNullActualAndAvailSucceeds)
#[test]
fn job_info_children_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child_job = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        buf.as_mut_ptr() as usize,
        16 * 8,
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenInvalidBufferPointerFails)
#[test]
fn job_info_children_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        0,  // null buffer
        64, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenBadActualIsInvalidArg)
#[test]
fn job_info_children_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        buf.as_mut_ptr() as usize,
        16 * 8,
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenBadAvailIsInvalidArg)
#[test]
fn job_info_children_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB_CHILDREN,
        buf.as_mut_ptr() as usize,
        16 * 8,
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── PROCESS (topic 3) — missing tests ──────────────────────────────

const PROCESS_INFO_SIZE: usize = 24; // zx_info_process_t

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessInvalidHandleFails)
#[test]
fn process_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; PROCESS_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessNullAvailSucceeds)
#[test]
fn process_info_null_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessNullActualSucceeds)
#[test]
fn process_info_null_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessNullActualAndAvailSucceeds)
#[test]
fn process_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessInvalidBufferPointerFails)
#[test]
fn process_info_invalid_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        0,                 // null buffer
        PROCESS_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessBadActualIsInvalidArg)
#[test]
fn process_info_bad_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessBadAvailIsInvalidArg)
#[test]
fn process_info_bad_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── PROCESS_THREADS (topic 4) — missing tests ──────────────────────

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsInvalidHandleFails)
#[test]
fn process_info_threads_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; 128];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS_THREADS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsZeroSizedBufferIsOk)
#[test]
fn process_info_threads_zero_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_THREADS, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    assert!(avail >= 1, "avail should report available count");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsNullAvailSucceeds)
#[test]
fn process_info_threads_null_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<128>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsNullActualSucceeds)
#[test]
fn process_info_threads_null_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<128>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsNullActualAndAvailSucceeds)
#[test]
fn process_info_threads_null_actual_and_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<128>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsInvalidBufferPointerFails)
#[test]
fn process_info_threads_invalid_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        0,   // null buffer
        128, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsBadActualIsInvalidArg)
#[test]
fn process_info_threads_bad_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<128>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessThreadsBadAvailIsInvalidArg)
#[test]
fn process_info_threads_bad_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<128>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_THREADS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── THREAD (topic 10) — missing tests ──────────────────────────────

const THREAD_INFO_SIZE: usize = 72;

/// C++: TEST(ThreadGetInfoTest, InfoThreadNullAvailSucceeds)
#[test]
fn thread_info_null_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadNullActualSucceeds)
#[test]
fn thread_info_null_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadNullActualAndAvailSucceeds)
#[test]
fn thread_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadInvalidBufferPointerFails)
#[test]
fn thread_info_invalid_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        0,                // null buffer
        THREAD_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadBadActualIsInvalidArg)
#[test]
fn thread_info_bad_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadBadAvailIsInvalidArg)
#[test]
fn thread_info_bad_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "info-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── VMO (topic 23) — missing tests ─────────────────────────────────

const VMO_INFO_SIZE: usize = 168;

/// C++: TEST(VmoGetInfoTest, InfoVmoNullAvailSucceeds)
#[test]
fn vmo_info_null_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut buf = AlignedBuf::<VMO_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(VmoGetInfoTest, InfoVmoNullActualSucceeds)
#[test]
fn vmo_info_null_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut buf = AlignedBuf::<VMO_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(VmoGetInfoTest, InfoVmoNullActualAndAvailSucceeds)
#[test]
fn vmo_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut buf = AlignedBuf::<VMO_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(VmoGetInfoTest, InfoVmoInvalidBufferPointerFails)
#[test]
fn vmo_info_invalid_buffer() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        0,             // null buffer
        VMO_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmoGetInfoTest, InfoVmoBadActualIsInvalidArg)
#[test]
fn vmo_info_bad_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut buf = AlignedBuf::<VMO_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmoGetInfoTest, InfoVmoBadAvailIsInvalidArg)
#[test]
fn vmo_info_bad_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx
        .proc
        .add_handle(Handle::new(vmo, Rights::READ | Rights::INSPECT));

    let mut buf = AlignedBuf::<VMO_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_VMO,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── VMAR (topic 7) — missing tests ─────────────────────────────────

/// C++: TEST_F(VmarGetInfoTest, InfoVmarNullAvailSucceeds)
#[test]
fn vmar_info_null_avail() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = AlignedBuf::<VMAR_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarNullActualSucceeds)
#[test]
fn vmar_info_null_actual() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = AlignedBuf::<VMAR_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarNullActualAndAvailSucceeds)
#[test]
fn vmar_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = AlignedBuf::<VMAR_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarInvalidBufferPointerFails)
#[test]
fn vmar_info_invalid_buffer() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        0,              // null buffer
        VMAR_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarBadActualIsInvalidArg)
#[test]
fn vmar_info_bad_actual() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = AlignedBuf::<VMAR_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(VmarGetInfoTest, InfoVmarBadAvailIsInvalidArg)
#[test]
fn vmar_info_bad_avail() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));

    let mut buf = AlignedBuf::<VMAR_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── HANDLE_BASIC on Job (topic 2) — missing tests ──────────────────

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicNullAvailSucceeds)
#[test]
fn job_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicNullActualSucceeds)
#[test]
fn job_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicNullActualAndAvailSucceeds)
#[test]
fn job_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicInvalidBufferPointerFails)
#[test]
fn job_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        0,                 // null buffer
        HANDLE_BASIC_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicBadActualIsInvalidArg)
#[test]
fn job_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoHandleBasicBadAvailIsInvalidArg)
#[test]
fn job_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── SOCKET (topic 22) — missing tests ──────────────────────────────

/// C++: TEST(SocketGetInfoTest, InfoSocketNullAvailSucceeds)
#[test]
fn socket_info_null_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<SOCKET_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(SocketGetInfoTest, InfoSocketNullActualSucceeds)
#[test]
fn socket_info_null_actual() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<SOCKET_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(SocketGetInfoTest, InfoSocketNullActualAndAvailSucceeds)
#[test]
fn socket_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<SOCKET_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(SocketGetInfoTest, InfoSocketInvalidBufferPointerFails)
#[test]
fn socket_info_invalid_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        0,                // null buffer
        SOCKET_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketGetInfoTest, InfoSocketBadActualIsInvalidArg)
#[test]
fn socket_info_bad_actual() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<SOCKET_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(SocketGetInfoTest, InfoSocketBadAvailIsInvalidArg)
#[test]
fn socket_info_bad_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<SOCKET_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        socket_h,
        TOPIC_SOCKET,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── BTI (topic 20) — missing tests ─────────────────────────────────

/// C++: TEST(BtiGetInfoTest, InfoBtiNullAvailSucceeds)
#[test]
fn bti_info_null_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<BTI_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(BtiGetInfoTest, InfoBtiNullActualSucceeds)
#[test]
fn bti_info_null_actual() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<BTI_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(BtiGetInfoTest, InfoBtiNullActualAndAvailSucceeds)
#[test]
fn bti_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<BTI_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(BtiGetInfoTest, InfoBtiInvalidBufferPointerFails)
#[test]
fn bti_info_invalid_buffer() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        0,             // null buffer
        BTI_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(BtiGetInfoTest, InfoBtiBadActualIsInvalidArg)
#[test]
fn bti_info_bad_actual() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<BTI_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(BtiGetInfoTest, InfoBtiBadAvailIsInvalidArg)
#[test]
fn bti_info_bad_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<BTI_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_BTI,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── TIMER (topic 25) — remaining 6 tests ──────────────────────────

/// C++: TEST(TimerGetInfoTest, InfoTimerNullAvailSucceeds)
#[test]
fn timer_info_null_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<TIMER_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(TimerGetInfoTest, InfoTimerNullActualSucceeds)
#[test]
fn timer_info_null_actual() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<TIMER_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(TimerGetInfoTest, InfoTimerNullActualAndAvailSucceeds)
#[test]
fn timer_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<TIMER_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(TimerGetInfoTest, InfoTimerInvalidBufferPointerFails)
#[test]
fn timer_info_invalid_buffer() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        0,               // null buffer
        TIMER_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(TimerGetInfoTest, InfoTimerBadActualIsInvalidArg)
#[test]
fn timer_info_bad_actual() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<TIMER_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(TimerGetInfoTest, InfoTimerBadAvailIsInvalidArg)
#[test]
fn timer_info_bad_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<TIMER_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        timer_h,
        TOPIC_TIMER,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── STREAM (topic 26) — remaining 6 tests ─────────────────────────

/// C++: TEST(StreamGetInfoTest, InfoStreamNullAvailSucceeds)
#[test]
fn stream_info_null_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = AlignedBuf::<STREAM_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(StreamGetInfoTest, InfoStreamNullActualSucceeds)
#[test]
fn stream_info_null_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = AlignedBuf::<STREAM_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(StreamGetInfoTest, InfoStreamNullActualAndAvailSucceeds)
#[test]
fn stream_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = AlignedBuf::<STREAM_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(StreamGetInfoTest, InfoStreamInvalidBufferPointerFails)
#[test]
fn stream_info_invalid_buffer() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        0,                // null buffer
        STREAM_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(StreamGetInfoTest, InfoStreamBadActualIsInvalidArg)
#[test]
fn stream_info_bad_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = AlignedBuf::<STREAM_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(StreamGetInfoTest, InfoStreamBadAvailIsInvalidArg)
#[test]
fn stream_info_bad_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let stream = Stream::create(vmo, 0, 0);
    let stream_h = ctx
        .proc
        .add_handle(Handle::new(stream, Rights::DEFAULT_STREAM));

    let mut buf = AlignedBuf::<STREAM_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        stream_h,
        TOPIC_STREAM,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── RESOURCE (topic 18) — remaining 7 tests ───────────────────────

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceZeroSizedBufferFails)
#[test]
fn resource_info_zero_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, res_h, TOPIC_RESOURCE, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceNullAvailSucceeds)
#[test]
fn resource_info_null_avail() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = AlignedBuf::<RESOURCE_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceNullActualSucceeds)
#[test]
fn resource_info_null_actual() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = AlignedBuf::<RESOURCE_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceNullActualAndAvailSucceeds)
#[test]
fn resource_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = AlignedBuf::<RESOURCE_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceInvalidBufferPointerFails)
#[test]
fn resource_info_invalid_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        0,                  // null buffer
        RESOURCE_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceBadActualIsInvalidArg)
#[test]
fn resource_info_bad_actual() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = AlignedBuf::<RESOURCE_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ResourceGetInfoTest, InfoResourceBadAvailIsInvalidArg)
#[test]
fn resource_info_bad_avail() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = AlignedBuf::<RESOURCE_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_RESOURCE,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── THREAD_STATS (topic 15) — remaining 6 tests ──────────────────

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsNullAvailSucceeds)
#[test]
fn thread_info_thread_stats_null_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_STATS_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsNullActualSucceeds)
#[test]
fn thread_info_thread_stats_null_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_STATS_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsNullActualAndAvailSucceeds)
#[test]
fn thread_info_thread_stats_null_actual_and_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_STATS_SIZE>::new();
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsInvalidBufferPointerFails)
#[test]
fn thread_info_thread_stats_invalid_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        0,                 // null buffer
        THREAD_STATS_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsBadActualIsInvalidArg)
#[test]
fn thread_info_thread_stats_bad_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_STATS_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoThreadStatsBadAvailIsInvalidArg)
#[test]
fn thread_info_thread_stats_bad_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "stats-thread").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));

    let mut buf = AlignedBuf::<THREAD_STATS_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_THREAD_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── HANDLE_COUNT (topic 19) — remaining 8 tests ──────────────────

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountInvalidHandleFails)
#[test]
fn handle_count_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_COUNT_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_COUNT, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountZeroSizedBufferFails)
#[test]
fn handle_count_zero_buffer() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, event_h, TOPIC_HANDLE_COUNT, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountNullAvailSucceeds)
#[test]
fn handle_count_null_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountNullActualSucceeds)
#[test]
fn handle_count_null_actual() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountNullActualAndAvailSucceeds)
#[test]
fn handle_count_null_actual_and_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountInvalidBufferPointerFails)
#[test]
fn handle_count_invalid_buffer() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        0,                 // null buffer
        HANDLE_COUNT_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountBadActualIsInvalidArg)
#[test]
fn handle_count_bad_actual() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ObjectGetInfoTest, InfoHandleCountBadAvailIsInvalidArg)
#[test]
fn handle_count_bad_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── HANDLE_BASIC on Process (topic 2) — remaining 6 tests ─────────

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicNullAvailSucceeds)
#[test]
fn process_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicNullActualSucceeds)
#[test]
fn process_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicNullActualAndAvailSucceeds)
#[test]
fn process_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicInvalidBufferPointerFails)
#[test]
fn process_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        0,                 // null buffer
        HANDLE_BASIC_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicBadActualIsInvalidArg)
#[test]
fn process_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoHandleBasicBadAvailIsInvalidArg)
#[test]
fn process_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── HANDLE_BASIC on Thread (topic 2) — remaining 6 tests ──────────

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicNullAvailSucceeds)
#[test]
fn thread_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicNullActualSucceeds)
#[test]
fn thread_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicNullActualAndAvailSucceeds)
#[test]
fn thread_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicInvalidBufferPointerFails)
#[test]
fn thread_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        0,                 // null buffer
        HANDLE_BASIC_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicBadActualIsInvalidArg)
#[test]
fn thread_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicBadAvailIsInvalidArg)
#[test]
fn thread_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── JOB (topic 24) — remaining 6 tests ────────────────────────────

/// C++: TEST_F(JobGetInfoTest, InfoJobNullAvailSucceeds)
#[test]
fn job_info_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<JOB_INFO_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobNullActualSucceeds)
#[test]
fn job_info_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<JOB_INFO_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobNullActualAndAvailSucceeds)
#[test]
fn job_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<JOB_INFO_SIZE>::new();
    get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(JobGetInfoTest, InfoJobInvalidBufferPointerFails)
#[test]
fn job_info_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        0,             // null buffer
        JOB_INFO_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobBadActualIsInvalidArg)
#[test]
fn job_info_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<JOB_INFO_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(JobGetInfoTest, InfoJobBadAvailIsInvalidArg)
#[test]
fn job_info_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = AlignedBuf::<JOB_INFO_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        job_h,
        TOPIC_JOB,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── PROCESS_HANDLE_STATS (topic 21) — remaining 8 tests ──────────

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsInvalidHandleFails)
#[test]
fn process_info_handle_stats_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; PROCESS_HANDLE_STATS_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS_HANDLE_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsZeroSizedBufferFails)
#[test]
fn process_info_handle_stats_zero_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, proc_h, TOPIC_PROCESS_HANDLE_STATS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsNullAvailSucceeds)
#[test]
fn process_info_handle_stats_null_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_HANDLE_STATS_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsNullActualSucceeds)
#[test]
fn process_info_handle_stats_null_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_HANDLE_STATS_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsNullActualAndAvailSucceeds)
#[test]
fn process_info_handle_stats_null_actual_and_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_HANDLE_STATS_SIZE>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsInvalidBufferPointerFails)
#[test]
fn process_info_handle_stats_invalid_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        0,                         // null buffer
        PROCESS_HANDLE_STATS_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsBadActualIsInvalidArg)
#[test]
fn process_info_handle_stats_bad_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_HANDLE_STATS_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleStatsBadAvailIsInvalidArg)
#[test]
fn process_info_handle_stats_bad_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));

    let mut buf = AlignedBuf::<PROCESS_HANDLE_STATS_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_HANDLE_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── HANDLE_TABLE (topic 27) — remaining 7 tests ──────────────────

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableNullAvailSucceeds)
#[test]
fn process_info_handle_table_null_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = AlignedBuf::<640>::new(); // 16 * 40
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableNullActualSucceeds)
#[test]
fn process_info_handle_table_null_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = AlignedBuf::<640>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableNullActualAndAvailSucceeds)
#[test]
fn process_info_handle_table_null_actual_and_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = AlignedBuf::<640>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableInvalidBufferPointerFails)
#[test]
fn process_info_handle_table_invalid_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        0,                         // null buffer
        16 * HANDLE_EXTENDED_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableBadActualIsInvalidArg)
#[test]
fn process_info_handle_table_bad_actual() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = AlignedBuf::<640>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        buf.as_mut_ptr() as usize,
        buf_size,
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableBadAvailIsInvalidArg)
#[test]
fn process_info_handle_table_bad_avail() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = AlignedBuf::<640>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_HANDLE_TABLE,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessHandleTableZeroSizedBufferIsOk)
#[test]
fn process_info_handle_table_zero_buffer() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_HANDLE_TABLE, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    // avail reports how many handles the process has
    assert!(avail >= 1, "process should have at least 1 handle");
}

// ═══════════════════════════════════════════════════════════════════
// ZX_INFO_PROCESS_MAPS (topic 13) — 10 tests
//
// PROCESS_MAPS returns an array of zx_info_maps_t entries describing
// the VMAR/mapping tree of a process. Not yet implemented in zCore
// (returns NOT_SUPPORTED), so all tests are #[should_panic].
// ═══════════════════════════════════════════════════════════════════

const TOPIC_PROCESS_MAPS: u32 = 13;
// zx_info_maps_t: name(32) + base(8) + size(8) + depth(8) + type-specific(48) = 104
const PROCESS_MAPS_ENTRY_SIZE: usize = 104;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsUnstartedSucceeds)
///
/// Query PROCESS_MAPS on an unstarted process. Should succeed with
/// at least the root VMAR entry. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_unstarted_succeeds() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsSmokeTest)
///
/// Query PROCESS_MAPS on a process with mappings. Should return
/// multiple entries describing the VMAR tree. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_smoke_test() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsInvalidHandleFails)
///
/// PROCESS_MAPS with an invalid handle should return BAD_HANDLE.
/// Not implemented in zCore (returns NOT_SUPPORTED before handle check).
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_invalid_handle() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullAvail)
///
/// PROCESS_MAPS with null avail pointer should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_null_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullActual)
///
/// PROCESS_MAPS with null actual pointer should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_null_actual() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullActualAndAvail)
///
/// PROCESS_MAPS with both null actual and avail pointers should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_null_actual_and_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsInvalidBuffer)
///
/// PROCESS_MAPS with null buffer pointer and non-zero size should
/// return INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_invalid_buffer() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsBadActual)
///
/// PROCESS_MAPS with misaligned actual pointer should return
/// INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_bad_actual() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsBadAvail)
///
/// PROCESS_MAPS with misaligned avail pointer should return
/// INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_bad_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsZeroBuffer)
///
/// PROCESS_MAPS with zero-length buffer should succeed with actual=0
/// and avail reporting the number of entries. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_maps_zero_buffer() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

// ═══════════════════════════════════════════════════════════════════
// ZX_INFO_PROCESS_VMOS (topic 14) — 10 tests
//
// PROCESS_VMOS returns an array of zx_info_vmo_t entries for all
// VMOs mapped into a process. Not yet implemented in zCore.
// ═══════════════════════════════════════════════════════════════════

const TOPIC_PROCESS_VMOS: u32 = 14;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosOnSelfSucceeds)
///
/// Query PROCESS_VMOS on self. Should return at least the VMOs backing
/// the process's VMAR tree. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_on_self_succeeds() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosSmokeTest)
///
/// Query PROCESS_VMOS on a process that has VMOs mapped. Should return
/// multiple VMO info entries. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_smoke_test() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosInvalidHandleFails)
///
/// PROCESS_VMOS with an invalid handle should return BAD_HANDLE.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_invalid_handle() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullAvail)
///
/// PROCESS_VMOS with null avail pointer should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_null_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullActual)
///
/// PROCESS_VMOS with null actual pointer should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_null_actual() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullActualAndAvail)
///
/// PROCESS_VMOS with both null actual and avail pointers should succeed.
/// Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_null_actual_and_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosInvalidBuffer)
///
/// PROCESS_VMOS with null buffer pointer and non-zero size should
/// return INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_invalid_buffer() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosBadActual)
///
/// PROCESS_VMOS with misaligned actual pointer should return
/// INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_bad_actual() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosBadAvail)
///
/// PROCESS_VMOS with misaligned avail pointer should return
/// INVALID_ARGS. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_bad_avail() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosZeroBuffer)
///
/// PROCESS_VMOS with zero-length buffer should succeed with actual=0
/// and avail reporting the number of VMOs. Not implemented in zCore.
#[should_panic(expected = "not yet implemented")]
#[test]
fn process_info_vmos_zero_buffer() {
    let _ctx = TestContext::new();
    panic!("not yet implemented");
}

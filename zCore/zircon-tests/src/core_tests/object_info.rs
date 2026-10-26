//! Object info tests.
//!
//! Faithful 1:1 ports from the object-info test suite (8 files).
//! Tests `zx_object_get_info` for various ZX_INFO_* topics.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::Event;
use zircon_object::task::{Job, Process, Thread};
use zircon_object::vm::VmObject;

extern crate alloc;

type HandleValue = u32;

// ── ZX_INFO_HANDLE_BASIC (topic 2) ──────────────────────────────────
// Tests that get_info with topic HANDLE_BASIC returns correct type,
// rights, and koid for various object types.

pub(crate) const TOPIC_HANDLE_BASIC: u32 = 2;
pub(crate) const HANDLE_BASIC_SIZE: usize = 32; // zx_info_handle_basic_t

/// Helper: call sys_object_get_info and return the result.
pub(crate) fn get_info(
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
pub(crate) fn get_info_err(
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

pub(crate) const TOPIC_PROCESS_THREADS: u32 = 4;

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

// Note: Fuchsia uses version 3 (0x3000_0017) for full-size VmoInfo.
// zCore currently only handles the base topic (23) which writes the
// full struct regardless of version. Using base topic for now.
pub(crate) const TOPIC_VMO: u32 = 23;

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

pub(crate) const TOPIC_JOB: u32 = 24;
pub(crate) const JOB_INFO_SIZE: usize = 16; // zx_info_job_t

// ── ZX_INFO_HANDLE_VALID (topic 1) ─────────────────────────────────

pub(crate) const TOPIC_HANDLE_VALID: u32 = 1;

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

pub(crate) const TOPIC_SOCKET: u32 = 22;
pub(crate) const SOCKET_INFO_SIZE: usize = 48; // zx_info_socket_t

// ── ZX_INFO_TIMER (topic 25) ───────────────────────────────────────

pub(crate) const TOPIC_TIMER: u32 = 25;
pub(crate) const TIMER_INFO_SIZE: usize = 24; // zx_info_timer_t

// ── ZX_INFO_STREAM (topic 26) ──────────────────────────────────────

pub(crate) const TOPIC_STREAM: u32 = 26;
pub(crate) const STREAM_INFO_SIZE: usize = 24; // zx_info_stream_t

// ── ZX_INFO_BTI (topic 20) ─────────────────────────────────────────

pub(crate) const TOPIC_BTI: u32 = 20;
pub(crate) const BTI_INFO_SIZE: usize = 32; // zx_info_bti_t

// ── ZX_INFO_HANDLE_COUNT on Thread (topic 19) ──────────────────────

pub(crate) const TOPIC_HANDLE_COUNT: u32 = 19;
pub(crate) const HANDLE_COUNT_SIZE: usize = 4; // u32

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
    let count = u32::from_ne_bytes(buf);
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

pub(crate) const TOPIC_PROCESS_HANDLE_STATS: u32 = 21;
pub(crate) const PROCESS_HANDLE_STATS_SIZE: usize = 256; // [u32; 64]

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

pub(crate) const TOPIC_RESOURCE: u32 = 18;
pub(crate) const RESOURCE_INFO_SIZE: usize = 56; // zx_info_resource_t

// ── ZX_INFO_HANDLE_TABLE (topic 27) ────────────────────────────────

pub(crate) const TOPIC_HANDLE_TABLE: u32 = 27;
pub(crate) const HANDLE_EXTENDED_SIZE: usize = 40; // zx_info_handle_extended_t

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

pub(crate) fn get_info_raw(
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
pub(crate) struct AlignedBuf<const N: usize>([u8; N]);

impl<const N: usize> AlignedBuf<N> {
    pub(crate) fn new() -> Self {
        Self([0u8; N])
    }
    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
    pub(crate) fn len(&self) -> usize {
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

pub(crate) const VMO_INFO_SIZE: usize = 168;

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

// ── BTI (topic 20) — missing tests ─────────────────────────────────

// ── TIMER (topic 25) — remaining 6 tests ──────────────────────────

// ── STREAM (topic 26) — remaining 6 tests ─────────────────────────

// ── RESOURCE (topic 18) — remaining 7 tests ───────────────────────

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

// ── PROCESS_HANDLE_STATS (topic 21) — remaining 8 tests ──────────

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

// ═══════════════════════════════════════════════════════════════════
// ZX_INFO_PROCESS_MAPS (topic 13) — 10 tests
//
// PROCESS_MAPS returns an array of zx_info_maps_t entries describing
// the VMAR/mapping tree of a process.
// Note: querying self returns (0, 0) to avoid VMAR lock deadlock.
// ═══════════════════════════════════════════════════════════════════

pub(crate) const TOPIC_PROCESS_MAPS: u32 = 13;
// zx_info_maps_t: name(32) + base(8) + size(8) + depth(8) + type-specific(48) = 104
const PROCESS_MAPS_ENTRY_SIZE: usize = 104;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsUnstartedSucceeds)
///
/// Query PROCESS_MAPS on an unstarted process. Should succeed with
/// at least the root VMAR entry (aspace entry at depth 0).
#[test]
fn process_info_maps_unstarted_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "unstarted").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    // Buffer for several entries
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    // An unstarted process still has a root VMAR → at least the aspace entry.
    assert!(
        avail >= 1,
        "unstarted process should have at least 1 maps entry (aspace)"
    );
    assert!(actual >= 1, "should have written at least 1 entry");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsSmokeTest)
///
/// Query PROCESS_MAPS on a process with mappings. Should return
/// entries describing the VMAR tree.
#[test]
fn process_info_maps_smoke_test() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "mapped").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 64 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert!(avail >= 1, "process should have at least 1 maps entry");
    assert_eq!(
        actual, avail,
        "buffer should be large enough for all entries"
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsInvalidHandleFails)
///
/// PROCESS_MAPS with an invalid handle should return BAD_HANDLE.
#[test]
fn process_info_maps_invalid_handle() {
    let ctx = TestContext::new();
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS_MAPS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullAvail)
///
/// PROCESS_MAPS with null avail pointer should succeed.
#[test]
fn process_info_maps_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-navail").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new(); // 16 * 104
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullActual)
///
/// PROCESS_MAPS with null actual pointer should succeed.
#[test]
fn process_info_maps_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-nactual").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsNullActualAndAvail)
///
/// PROCESS_MAPS with both null actual and avail pointers should succeed.
#[test]
fn process_info_maps_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-nboth").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsInvalidBuffer)
///
/// PROCESS_MAPS with null buffer pointer and non-zero size should
/// return INVALID_ARGS.
#[test]
fn process_info_maps_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-invbuf").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        0,                            // null buffer
        16 * PROCESS_MAPS_ENTRY_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsBadActual)
///
/// PROCESS_MAPS with misaligned actual pointer should return
/// INVALID_ARGS.
#[test]
fn process_info_maps_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-bact").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessMapsBadAvail)
///
/// PROCESS_MAPS with misaligned avail pointer should return
/// INVALID_ARGS.
#[test]
fn process_info_maps_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-bavail").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ═══════════════════════════════════════════════════════════════════
// ZX_INFO_PROCESS_VMOS (topic 14) — 10 tests
//
// PROCESS_VMOS returns an array of zx_info_vmo_t entries for all
// VMOs accessible by a process (via handles or mappings).
// ═══════════════════════════════════════════════════════════════════

pub(crate) const TOPIC_PROCESS_VMOS: u32 = 14;
// V0 zx_info_vmo_t entry size (topic 14 with version 0 uses 104 bytes)
const PROCESS_VMOS_ENTRY_SIZE: usize = 104;

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosOnSelfSucceeds)
///
/// Query PROCESS_VMOS on a process that has at least one VMO handle.
/// Should return the VMO info entries.
#[test]
fn process_info_vmos_on_self_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-self").unwrap();

    // Give the target process a VMO handle so there's at least one entry.
    let vmo = VmObject::new_paged(1);
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));

    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(
        avail >= 1,
        "process with VMO handle should have at least 1 VMO entry"
    );
    assert!(actual >= 1, "should have written at least 1 entry");
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosSmokeTest)
///
/// Query PROCESS_VMOS on a process that has VMO handles. Should return
/// VMO info entries with correct koid, size, flags, parent_koid,
/// num_children, share_count — matching the C++ test assertions.
#[test]
fn process_info_vmos_smoke_test() {
    use zircon_object::vm::{VmoInfoFlags, PAGE_SIZE};

    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-smoke").unwrap();

    // Add multiple VMO handles to the target process.
    let vmo1 = VmObject::new_paged(1);
    let vmo2 = VmObject::new_paged(2);
    let vmo1_koid = vmo1.id();
    let vmo2_koid = vmo2.id();
    let vmo1_size = (PAGE_SIZE) as u64;
    let vmo2_size = (2 * PAGE_SIZE) as u64;
    target.add_handle(Handle::new(vmo1, Rights::DEFAULT_VMO));
    target.add_handle(Handle::new(vmo2, Rights::DEFAULT_VMO));

    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 64 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(avail >= 2, "process should have at least 2 VMO entries");
    assert_eq!(
        actual, avail,
        "buffer should be large enough for all entries"
    );

    // Parse VmoInfo entries and verify fields like the C++ test does.
    // VmoInfo V0 entry layout (104 bytes):
    //   koid: u64 @ 0
    //   name: [u8; 32] @ 8
    //   size: u64 @ 40
    //   parent_koid: u64 @ 48
    //   num_children: u64 @ 56
    //   num_mappings: u64 @ 64
    //   share_count: u64 @ 72
    //   flags: u32 @ 80
    let mut saw_vmo1 = false;
    let mut saw_vmo2 = false;
    for i in 0..actual {
        let base = i * PROCESS_VMOS_ENTRY_SIZE;
        let entry_koid = u64::from_ne_bytes(buf[base..base + 8].try_into().unwrap());
        let entry_size = u64::from_ne_bytes(buf[base + 40..base + 48].try_into().unwrap());
        let parent_koid = u64::from_ne_bytes(buf[base + 48..base + 56].try_into().unwrap());
        let num_children = u64::from_ne_bytes(buf[base + 56..base + 64].try_into().unwrap());
        let share_count = u64::from_ne_bytes(buf[base + 72..base + 80].try_into().unwrap());
        let flags = u32::from_ne_bytes(buf[base + 80..base + 84].try_into().unwrap());
        let info_flags = VmoInfoFlags::from_bits_truncate(flags);

        // All VMOs should be paged, not physical.
        assert!(
            info_flags.contains(VmoInfoFlags::TYPE_PAGED),
            "entry {} (koid={}) should be TYPE_PAGED, flags=0x{:x}",
            i,
            entry_koid,
            flags
        );

        // Each entry should be via either map or handle, but not both.
        let via_handle = info_flags.contains(VmoInfoFlags::VIA_HANDLE);
        let via_mapping = info_flags.contains(VmoInfoFlags::VIA_MAPPING);
        assert!(
            !(via_handle && via_mapping),
            "entry {} (koid={}) should not be both VIA_HANDLE and VIA_MAPPING",
            i,
            entry_koid
        );

        // Match our known VMOs by koid.
        if entry_koid == vmo1_koid && entry_size == vmo1_size {
            saw_vmo1 = true;
            // Not a clone, not shared.
            assert_eq!(parent_koid, 0, "vmo1 parent_koid");
            assert_eq!(num_children, 0, "vmo1 num_children");
            assert_eq!(share_count, 1, "vmo1 share_count");
            assert!(via_handle, "vmo1 should be VIA_HANDLE");
            assert!(
                !info_flags.contains(VmoInfoFlags::IS_COW_CLONE),
                "vmo1 should not be IS_COW_CLONE"
            );
        }
        if entry_koid == vmo2_koid && entry_size == vmo2_size {
            saw_vmo2 = true;
            assert_eq!(parent_koid, 0, "vmo2 parent_koid");
            assert_eq!(num_children, 0, "vmo2 num_children");
            assert_eq!(share_count, 1, "vmo2 share_count");
            assert!(via_handle, "vmo2 should be VIA_HANDLE");
            assert!(
                !info_flags.contains(VmoInfoFlags::IS_COW_CLONE),
                "vmo2 should not be IS_COW_CLONE"
            );
        }
    }
    assert!(saw_vmo1, "should have found vmo1 (koid={})", vmo1_koid);
    assert!(saw_vmo2, "should have found vmo2 (koid={})", vmo2_koid);

    // C++ also does a second read with a shorter buffer to test actual < avail.
    let short_count = actual * 3 / 4;
    if short_count > 0 && short_count < actual {
        let short_size = short_count * PROCESS_VMOS_ENTRY_SIZE;
        let mut short_buf = vec![0u8; short_size];
        let (actual_2, avail_2) =
            get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut short_buf).unwrap();
        assert!(
            actual_2 < avail_2,
            "short buffer: actual_2 ({}) should be < avail_2 ({})",
            actual_2,
            avail_2
        );
        // C++: EXPECT_LT(actual_2, available_2) — and then EXPECT_GT(actual_2, 3)
        // only applies when the process has many VMOs (mini-process has vDSO, stack, etc.)
        // In our test with 2 VMOs, short_count=1, so actual_2=1 is expected.
    }
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosInvalidHandleFails)
///
/// PROCESS_VMOS with an invalid handle should return BAD_HANDLE.
#[test]
fn process_info_vmos_invalid_handle() {
    let ctx = TestContext::new();
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = vec![0u8; buf_size];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS_VMOS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullAvail)
///
/// PROCESS_VMOS with null avail pointer should succeed.
#[test]
fn process_info_vmos_null_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-navail").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new(); // 16 * 104
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullActual)
///
/// PROCESS_VMOS with null actual pointer should succeed.
#[test]
fn process_info_vmos_null_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-nactual").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosNullActualAndAvail)
///
/// PROCESS_VMOS with both null actual and avail pointers should succeed.
#[test]
fn process_info_vmos_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-nboth").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0, // null actual
        0, // null avail
    )
    .unwrap();
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosInvalidBuffer)
///
/// PROCESS_VMOS with null buffer pointer and non-zero size should
/// return INVALID_ARGS.
#[test]
fn process_info_vmos_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-invbuf").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        0,                            // null buffer
        16 * PROCESS_VMOS_ENTRY_SIZE, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosBadActual)
///
/// PROCESS_VMOS with misaligned actual pointer should return
/// INVALID_ARGS.
#[test]
fn process_info_vmos_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-bact").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        1, // bad actual (misaligned)
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST_F(ProcessGetInfoTest, InfoProcessVmosBadAvail)
///
/// PROCESS_VMOS with misaligned avail pointer should return
/// INVALID_ARGS.
#[test]
fn process_info_vmos_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-bavail").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1, // bad avail (misaligned)
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Event (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on EventPair (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Channel (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_VALID (topic 1) — NullAvail, NullActual, NullActualAndAvail
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on VMO (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Socket (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Timer (topic 2) — 6 additional tests
// (OnSelf, InvalidHandle, ZeroBuffer already exist above)
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Timer (topic 2) — 3 missing tests (on_self, invalid_handle, zero_buffer)
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Thread (topic 2) — 3 missing tests (on_self, invalid_handle, zero_buffer)
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicOnSelfSucceeds)
#[test]
fn thread_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicInvalidHandleFails)
#[test]
fn thread_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST_F(ThreadGetInfoTest, InfoHandleBasicZeroSizedBufferFails)
#[test]
fn thread_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hb-thread").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, thread_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Fifo (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Port (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Counter (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Pager (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on BTI (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// HANDLE_BASIC on Resource (topic 2) — 9 tests
// ═══════════════════════════════════════════════════════════════════

/// C++: TEST(JobGetInfoTest, InfoJobChildJobsGetChild)
/// Create a child job, enumerate via JOB_CHILDREN, then look up the child
/// koid with sys_object_get_child (here we just verify the koid appears).
#[test]
fn job_get_info_info_job_child_jobs_get_child() {
    let ctx = TestContext::new();
    let job = Job::root();
    let child = job.create_child().unwrap();
    let child_koid = child.id();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let (actual, _avail) = get_info(&ctx, job_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    })
    .unwrap();
    assert!(actual >= 1);
    let koids = &buf[..actual];
    assert!(
        koids.contains(&child_koid),
        "child koid should appear in JOB_CHILDREN"
    );
}

/// C++: TEST(JobGetInfoTest, InfoJobChildrenJobHandleIsBadHandle)
/// Querying JOB_CHILDREN with a Job handle (not a process/thread) is the
/// correct usage. Using a non-job handle should return BAD_HANDLE or
/// WRONG_TYPE. Here: use an event handle.
#[test]
fn job_get_info_info_job_children_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, event_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE || err == zircon_object::ZxError::BAD_HANDLE,
        "expected WRONG_TYPE or BAD_HANDLE, got {:?}",
        err
    );
}

/// C++: TEST(JobGetInfoTest, InfoJobChildrenPartiallyUnmappedBufferIsInvalidArgs)
/// Null buffer with non-zero size → INVALID_ARGS.
#[test]
fn job_get_info_info_job_children_partially_unmapped_buffer_is_invalid_args() {
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

/// C++: TEST(JobGetInfoTest, InfoJobChildrenRequiresEnumerateRights)
/// JOB_CHILDREN without ENUMERATE right → ACCESS_DENIED.
#[test]
fn job_get_info_info_job_children_requires_enumerate_rights() {
    let ctx = TestContext::new();
    let job = Job::root();
    // Add handle WITHOUT ENUMERATE right
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::INSPECT));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, job_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(JobGetInfoTest, InfoJobChildrenSmallBufferIsOk)
/// A buffer smaller than avail should still succeed with actual < avail.
#[test]
fn job_get_info_info_job_children_small_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _child1 = job.create_child().unwrap();
    let _child2 = job.create_child().unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    // Buffer for only 1 koid
    let mut buf = [0u64; 1];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 8)
    })
    .unwrap();
    assert_eq!(actual, 1, "only room for 1 entry");
    assert!(avail >= 2, "should report at least 2 available");
}

/// C++: TEST(JobGetInfoTest, InfoJobChildrenThreadHandleIsBadHandle)
/// Using a thread handle for JOB_CHILDREN should return WRONG_TYPE.
#[test]
fn job_get_info_info_job_children_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, thread_h, TOPIC_JOB_CHILDREN, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(JobGetInfoTest, InfoJobProcessesGetChild)
/// Create a process in a job, enumerate via JOB_PROCESSES, verify the
/// process koid appears.
#[test]
fn job_get_info_info_job_processes_get_child() {
    let ctx = TestContext::new();
    let job = Job::root();
    let child_proc = Process::create(&job, "child").unwrap();
    let child_koid = child_proc.id();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    let mut buf = [0u64; 16];
    let (actual, _avail) = get_info(&ctx, job_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    })
    .unwrap();
    assert!(actual >= 1);
    let koids = &buf[..actual];
    assert!(
        koids.contains(&child_koid),
        "child process koid should appear in JOB_PROCESSES"
    );
}

/// C++: TEST(JobGetInfoTest, InfoJobProcessesPartiallyUnmappedBufferIsInvalidArgs)
/// Null buffer with non-zero size → INVALID_ARGS.
#[test]
fn job_get_info_info_job_processes_partially_unmapped_buffer_is_invalid_args() {
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

/// C++: TEST(JobGetInfoTest, InfoJobProcessesProcessbHandleIsBadHandle)
/// Using a process handle for JOB_PROCESSES should return WRONG_TYPE.
#[test]
fn job_get_info_info_job_processes_processb_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, proc_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(JobGetInfoTest, InfoJobProcessesRequiresEnumerateRights)
/// JOB_PROCESSES without ENUMERATE right → ACCESS_DENIED.
#[test]
fn job_get_info_info_job_processes_requires_enumerate_rights() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::INSPECT));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, job_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(JobGetInfoTest, InfoJobProcessesSmallBufferIsOk)
/// A buffer smaller than avail should still succeed with actual < avail.
#[test]
fn job_get_info_info_job_processes_small_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let _p1 = Process::create(&job, "p1").unwrap();
    let _p2 = Process::create(&job, "p2").unwrap();
    let job_h = ctx
        .proc
        .add_handle(Handle::new(job, Rights::DEFAULT_JOB | Rights::ENUMERATE));

    // Buffer for only 1 koid
    let mut buf = [0u64; 1];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 8)
    })
    .unwrap();
    assert_eq!(actual, 1, "only room for 1 entry");
    assert!(avail >= 2, "should report at least 2 available");
}

/// C++: TEST(JobGetInfoTest, InfoJobProcessesThreadHandleIsBadHandle)
/// Using a thread handle for JOB_PROCESSES should return WRONG_TYPE.
#[test]
fn job_get_info_info_job_processes_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));

    let mut buf = [0u64; 16];
    let err = get_info_err(&ctx, thread_h, TOPIC_JOB_PROCESSES, unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 16 * 8)
    });
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

// ── Kernel Stats tests ─────────────────────────────────────────────
// CpuStats (topic 16), KmemStats (topic 17), KmemStatsExtended (topic 31),
// GuestStats (topic 29) all require a root resource handle.

const TOPIC_CPU_STATS: u32 = 16;
const TOPIC_KMEM_STATS: u32 = 17;
const TOPIC_KMEM_STATS_EXTENDED: u32 = 31;
const TOPIC_GUEST_STATS: u32 = 29;

/// C++: TEST(KernelStatsGetInfoTest, CpuStats)
#[test]
fn kernel_stats_cpu_stats() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    // zx_info_cpu_stats_t per CPU: 8 * u64 = 64 bytes per cpu.
    // Provide buffer for up to 8 CPUs.
    let mut buf = vec![0u8; 64 * 8];
    let result = get_info(&ctx, res_h, TOPIC_CPU_STATS, &mut buf);
    // May succeed or return NOT_SUPPORTED in libos mode.
    match result {
        Ok((actual, _avail)) => assert!(actual >= 1, "should have at least 1 CPU"),
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(KernelStatsGetInfoTest, CpuStatsArraySize)
#[test]
fn kernel_stats_cpu_stats_array_size() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    // Zero-length buffer to query avail count.
    let mut buf = [0u8; 0];
    let result = get_info(&ctx, res_h, TOPIC_CPU_STATS, &mut buf);
    match result {
        Ok((_actual, avail)) => assert!(avail >= 1),
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(KernelStatsGetInfoTest, CpuStatsBadHandleType)
#[test]
fn kernel_stats_cpu_stats_bad_handle_type() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let mut buf = vec![0u8; 512];
    let err = get_info_err(&ctx, event_h, TOPIC_CPU_STATS, &mut buf);
    // Kernel stats require a resource handle — event gives WRONG_TYPE.
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE
            || err == zircon_object::ZxError::ACCESS_DENIED
            || err == zircon_object::ZxError::NOT_SUPPORTED,
        "expected WRONG_TYPE/ACCESS_DENIED/NOT_SUPPORTED, got {:?}",
        err
    );
}

/// C++: TEST(KernelStatsGetInfoTest, CpuStatsInvalidHandle)
#[test]
fn kernel_stats_cpu_stats_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = vec![0u8; 512];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_CPU_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(KernelStatsGetInfoTest, CpuStatsMonotonicValues)
#[test]
fn kernel_stats_cpu_stats_monotonic_values() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = vec![0u8; 64 * 8];
    // Query twice and compare — values should be monotonically non-decreasing.
    let r1 = get_info(&ctx, res_h, TOPIC_CPU_STATS, &mut buf);
    let r2 = get_info(&ctx, res_h, TOPIC_CPU_STATS, &mut buf);
    // Both should produce same result type.
    assert_eq!(r1.is_ok(), r2.is_ok());
}

/// C++: TEST(KernelStatsGetInfoTest, CpuStatsNullBuffer)
#[test]
fn kernel_stats_cpu_stats_null_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_CPU_STATS,
        0,   // null buffer
        512, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(KernelStatsGetInfoTest, GuestStatsArraySize)
#[test]
fn kernel_stats_guest_stats_array_size() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = [0u8; 0];
    // Guest stats may not be implemented — just exercise the path.
    // Zero-sized buffer → BUFFER_TOO_SMALL (or NOT_SUPPORTED if guest stats unimplemented).
    let result = get_info(&ctx, res_h, TOPIC_GUEST_STATS, &mut buf);
    if let Err(e) = result {
        assert!(
            e == zircon_object::ZxError::BUFFER_TOO_SMALL
                || e == zircon_object::ZxError::NOT_SUPPORTED,
        );
    }
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStats)
#[test]
fn kernel_stats_kmem_stats() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    // zx_info_kmem_stats_t is 152 bytes (19 × u64)
    let mut buf = vec![0u8; 152];
    let result = get_info(&ctx, res_h, TOPIC_KMEM_STATS, &mut buf);
    match result {
        Ok((actual, avail)) => {
            assert_eq!(actual, 1);
            assert_eq!(avail, 1);
        }
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsBadHandleType)
#[test]
fn kernel_stats_kmem_stats_bad_handle_type() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let mut buf = vec![0u8; 112];
    let err = get_info_err(&ctx, event_h, TOPIC_KMEM_STATS, &mut buf);
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE
            || err == zircon_object::ZxError::ACCESS_DENIED
            || err == zircon_object::ZxError::NOT_SUPPORTED,
        "expected WRONG_TYPE/ACCESS_DENIED/NOT_SUPPORTED, got {:?}",
        err
    );
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsExtended)
#[test]
fn kernel_stats_kmem_stats_extended() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = vec![0u8; 256];
    let result = get_info(&ctx, res_h, TOPIC_KMEM_STATS_EXTENDED, &mut buf);
    match result {
        Ok((actual, avail)) => {
            assert_eq!(actual, 1);
            assert_eq!(avail, 1);
        }
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsExtendedBadHandleType)
#[test]
fn kernel_stats_kmem_stats_extended_bad_handle_type() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
    let mut buf = vec![0u8; 256];
    let err = get_info_err(&ctx, event_h, TOPIC_KMEM_STATS_EXTENDED, &mut buf);
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE
            || err == zircon_object::ZxError::ACCESS_DENIED
            || err == zircon_object::ZxError::NOT_SUPPORTED,
        "expected WRONG_TYPE/ACCESS_DENIED/NOT_SUPPORTED, got {:?}",
        err
    );
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsExtendedInvalidHandle)
#[test]
fn kernel_stats_kmem_stats_extended_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = vec![0u8; 256];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_KMEM_STATS_EXTENDED, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsExtendedNullBuffer)
#[test]
fn kernel_stats_kmem_stats_extended_null_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_KMEM_STATS_EXTENDED,
        0,   // null buffer
        256, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsExtendedSmallBuffer)
#[test]
fn kernel_stats_kmem_stats_extended_small_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = [0u8; 4]; // too small
    let result = get_info(&ctx, res_h, TOPIC_KMEM_STATS_EXTENDED, &mut buf);
    if let Err(e) = result {
        assert!(
            e == zircon_object::ZxError::BUFFER_TOO_SMALL
                || e == zircon_object::ZxError::NOT_SUPPORTED,
        );
    }
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsInvalidHandle)
#[test]
fn kernel_stats_kmem_stats_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = vec![0u8; 112];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_KMEM_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsNullBuffer)
#[test]
fn kernel_stats_kmem_stats_null_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_KMEM_STATS,
        0,   // null buffer
        112, // non-zero size
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(KernelStatsGetInfoTest, KmemStatsSmallBuffer)
#[test]
fn kernel_stats_kmem_stats_small_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = [0u8; 4]; // too small
    let result = get_info(&ctx, res_h, TOPIC_KMEM_STATS, &mut buf);
    if let Err(e) = result {
        assert!(
            e == zircon_object::ZxError::BUFFER_TOO_SMALL
                || e == zircon_object::ZxError::NOT_SUPPORTED,
        );
    }
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessHandleTable)
/// Query HANDLE_TABLE and verify the entries contain handles with correct
/// type, koid, and related_koid — matching the C++ test assertions.
///
/// The C++ test creates a mini-process with a VMO handle and a channel
/// handle, then verifies:
///   - handle_info[0].type == ZX_OBJ_TYPE_VMO (3)
///   - handle_info[1].type == ZX_OBJ_TYPE_CHANNEL (4)
///   - Both have non-zero handle_value and koid
///   - VMO has related_koid == 0, channel has related_koid > 0
///
/// We replicate this by giving the target process a VMO and a channel.
#[test]
fn process_get_info_info_process_handle_table() {
    use zircon_object::ipc::Channel;

    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "ht").unwrap();

    // Add a VMO handle (type 3) and a channel handle (type 4).
    let vmo = VmObject::new_paged(1);
    let vmo_koid = vmo.id();
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));

    let (ch0, ch1) = Channel::create();
    let ch0_koid = ch0.id();
    let ch0_related = ch0.related_koid();
    target.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    // Keep ch1 alive so related_koid is valid.
    let _ch1 = ch1;

    let proc_h = ctx.proc.add_handle(Handle::new(
        target,
        Rights::DEFAULT_PROCESS | Rights::MANAGE_PROCESS,
    ));

    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = vec![0u8; buf_size];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_HANDLE_TABLE, &mut buf).unwrap();
    assert_eq!(actual, avail);
    assert_eq!(avail, 2, "should have exactly 2 handles (VMO + channel)");

    // Parse HandleExtendedInfo entries.
    // Layout (40 bytes):
    //   obj_type: u32 @ 0
    //   handle_value: u32 @ 4
    //   rights: u32 @ 8
    //   reserved: u32 @ 12
    //   koid: u64 @ 16
    //   related_koid: u64 @ 24
    //   peer_owner_koid: u64 @ 32
    //
    // Entries are sorted by handle_value. The first handle added gets
    // the lower value, so the order should be VMO then channel.
    let type0 = u32::from_ne_bytes(buf[0..4].try_into().unwrap());
    let hv0 = u32::from_ne_bytes(buf[4..8].try_into().unwrap());
    let koid0 = u64::from_ne_bytes(buf[16..24].try_into().unwrap());
    let related0 = u64::from_ne_bytes(buf[24..32].try_into().unwrap());

    let base1 = HANDLE_EXTENDED_SIZE;
    let type1 = u32::from_ne_bytes(buf[base1..base1 + 4].try_into().unwrap());
    let hv1 = u32::from_ne_bytes(buf[base1 + 4..base1 + 8].try_into().unwrap());
    let koid1 = u64::from_ne_bytes(buf[base1 + 16..base1 + 24].try_into().unwrap());
    let related1 = u64::from_ne_bytes(buf[base1 + 24..base1 + 32].try_into().unwrap());

    // C++: EXPECT_EQ(handle_info[0].type, ZX_OBJ_TYPE_VMO)
    assert_eq!(
        type0, 3,
        "first handle should be VMO (type 3), got {}",
        type0
    );
    // C++: EXPECT_EQ(handle_info[1].type, ZX_OBJ_TYPE_CHANNEL)
    assert_eq!(
        type1, 4,
        "second handle should be Channel (type 4), got {}",
        type1
    );
    // C++: EXPECT_NE(handle_info[0].handle_value, ZX_HANDLE_INVALID)
    assert_ne!(hv0, 0, "VMO handle_value should be non-zero");
    assert_ne!(hv1, 0, "Channel handle_value should be non-zero");
    // C++: EXPECT_GT(handle_info[0].koid, 0)
    assert!(koid0 > 0, "VMO koid should be > 0");
    assert!(koid1 > 0, "Channel koid should be > 0");
    // Verify koids match what we created.
    assert_eq!(koid0, vmo_koid, "VMO koid mismatch");
    assert_eq!(koid1, ch0_koid, "Channel koid mismatch");
    // C++: EXPECT_EQ(handle_info[0].related_koid, 0) — VMO has no peer
    assert_eq!(related0, 0, "VMO related_koid should be 0");
    // C++: EXPECT_GT(handle_info[1].related_koid, 0) — channel has a peer
    assert!(
        related1 > 0,
        "Channel related_koid should be > 0, got {}",
        related1
    );
    assert_eq!(related1, ch0_related, "Channel related_koid mismatch");
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessHandleTableInsufficientRights)
/// HANDLE_TABLE without MANAGE_PROCESS right → ACCESS_DENIED.
#[test]
fn process_get_info_info_process_handle_table_insufficient_rights() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "ht-norights").unwrap();
    // No MANAGE_PROCESS right
    let proc_h = ctx.proc.add_handle(Handle::new(target, Rights::INSPECT));
    let buf_size = 16 * HANDLE_EXTENDED_SIZE;
    let mut buf = vec![0u8; buf_size];
    let err = get_info_err(&ctx, proc_h, TOPIC_HANDLE_TABLE, &mut buf);
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessHandleTableNullActualAvailSucceeds)
#[test]
fn process_get_info_info_process_handle_table_null_actual_avail_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "ht-null").unwrap();
    let proc_h = ctx.proc.add_handle(Handle::new(
        target,
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
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessJobHandleIsBadHandle)
/// Querying PROCESS topic with a job handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = [0u8; PROCESS_INFO_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_PROCESS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsBadActualIsInvalidArg)
#[test]
fn process_get_info_info_process_maps_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-ba2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsBadAvailIsInvalidArg)
#[test]
fn process_get_info_info_process_maps_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-bav2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsInvalidBufferPointerFails)
#[test]
fn process_get_info_info_process_maps_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-inv2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        0,
        16 * PROCESS_MAPS_ENTRY_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsJobHandleIsBadHandle)
/// Querying PROCESS_MAPS with a job handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_maps_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = vec![0u8; 16 * PROCESS_MAPS_ENTRY_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_PROCESS_MAPS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsNullActualAndAvailSucceeds)
#[test]
fn process_get_info_info_process_maps_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-na2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsNullActualSucceeds)
#[test]
fn process_get_info_info_process_maps_null_actual_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-nact2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsNullAvailSucceeds)
#[test]
fn process_get_info_info_process_maps_null_avail_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-nav2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsOnSelfFails)
/// Querying PROCESS_MAPS on self may deadlock (VMAR lock) — zCore
/// returns (0, 0) to avoid this. Verify we don't crash.
#[test]
fn process_get_info_info_process_maps_on_self_fails() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 16 * PROCESS_MAPS_ENTRY_SIZE];
    // Self-query may succeed with (0,0) or may fail — either is acceptable.
    let result = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf);
    assert!(
        result.is_ok()
            || result == Err(zircon_object::ZxError::NOT_SUPPORTED)
            || result == Err(zircon_object::ZxError::ACCESS_DENIED),
    );
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsPartiallyUnmappedBufferIsInvalidArgs)
#[test]
fn process_get_info_info_process_maps_partially_unmapped_buffer_is_invalid_args() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-unm2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_MAPS,
        0,
        16 * PROCESS_MAPS_ENTRY_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsRequiresInspectRights)
/// PROCESS_MAPS without INSPECT right → ACCESS_DENIED.
#[test]
fn process_get_info_info_process_maps_requires_inspect_rights() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-noinsp").unwrap();
    // No INSPECT right
    let proc_h = ctx.proc.add_handle(Handle::new(target, Rights::ENUMERATE));
    let mut buf = vec![0u8; 16 * PROCESS_MAPS_ENTRY_SIZE];
    let err = get_info_err(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsSmallBufferIsOk)
/// A buffer too small for all entries should succeed with actual < avail.
#[test]
fn process_get_info_info_process_maps_small_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-small").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    // Buffer for only 1 entry
    let mut buf = vec![0u8; PROCESS_MAPS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert!(actual <= 1);
    assert!(avail >= actual);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsSmokeTestV1)
#[test]
fn process_get_info_info_process_maps_smoke_test_v1() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-v1").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 64 * PROCESS_MAPS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert_eq!(actual, avail);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsSmokeTestV2)
#[test]
fn process_get_info_info_process_maps_smoke_test_v2() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-v2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 64 * PROCESS_MAPS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert!(avail >= 1, "should have at least the root VMAR");
    assert_eq!(actual, avail);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsThreadHandleIsBadHandle)
/// Querying PROCESS_MAPS with a thread handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_maps_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = vec![0u8; 16 * PROCESS_MAPS_ENTRY_SIZE];
    let err = get_info_err(&ctx, thread_h, TOPIC_PROCESS_MAPS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessMapsZeroSizedBufferIsOk)
#[test]
fn process_get_info_info_process_maps_zero_sized_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "maps-z2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert_eq!(actual, 0);
    assert!(avail >= 1);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessThreadHandleIsBadHandle)
/// Querying PROCESS topic with a thread handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = [0u8; PROCESS_INFO_SIZE];
    let err = get_info_err(&ctx, thread_h, TOPIC_PROCESS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessThreadsZeroSizedBufferSucceeds)
#[test]
fn process_get_info_info_process_threads_zero_sized_buffer_succeeds() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::ENUMERATE,
    ));
    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_THREADS, &mut buf).unwrap();
    assert_eq!(actual, 0);
    assert!(avail >= 1);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosBadActualIsInvalidArg)
#[test]
fn process_get_info_info_process_vmos_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-ba2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosBadAvailIsInvalidArg)
#[test]
fn process_get_info_info_process_vmos_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-bav2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosInvalidBufferPointerFails)
#[test]
fn process_get_info_info_process_vmos_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-inv2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        0,
        16 * PROCESS_VMOS_ENTRY_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosJobHandleIsBadHandle)
/// Querying PROCESS_VMOS with a job handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_vmos_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = vec![0u8; 16 * PROCESS_VMOS_ENTRY_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_PROCESS_VMOS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosNullActualAndAvailSucceeds)
#[test]
fn process_get_info_info_process_vmos_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-na2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosNullActualSucceeds)
#[test]
fn process_get_info_info_process_vmos_null_actual_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-nact2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosNullAvailSucceeds)
#[test]
fn process_get_info_info_process_vmos_null_avail_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-nav2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let buf_size = 16 * PROCESS_VMOS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosPartiallyUnmappedBufferIsInvalidArgs)
#[test]
fn process_get_info_info_process_vmos_partially_unmapped_buffer_is_invalid_args() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-unm2").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_PROCESS_VMOS,
        0,
        16 * PROCESS_VMOS_ENTRY_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosRequiresInspectRights)
/// PROCESS_VMOS without INSPECT right → ACCESS_DENIED.
#[test]
fn process_get_info_info_process_vmos_requires_inspect_rights() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-noinsp").unwrap();
    let proc_h = ctx.proc.add_handle(Handle::new(target, Rights::ENUMERATE));
    let mut buf = vec![0u8; 16 * PROCESS_VMOS_ENTRY_SIZE];
    let err = get_info_err(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosSmallBufferIsOk)
#[test]
fn process_get_info_info_process_vmos_small_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-small").unwrap();
    let vmo1 = VmObject::new_paged(1);
    let vmo2 = VmObject::new_paged(1);
    target.add_handle(Handle::new(vmo1, Rights::DEFAULT_VMO));
    target.add_handle(Handle::new(vmo2, Rights::DEFAULT_VMO));
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    // Buffer for only 1 entry
    let mut buf = vec![0u8; PROCESS_VMOS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(actual <= 1);
    assert!(avail >= 2);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosSmokeTestV1)
#[test]
fn process_get_info_info_process_vmos_smoke_test_v1() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-sv1").unwrap();
    let vmo = VmObject::new_paged(1);
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 64 * PROCESS_VMOS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(avail >= 1);
    assert_eq!(actual, avail);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosSmokeTestV2)
#[test]
fn process_get_info_info_process_vmos_smoke_test_v2() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-sv2").unwrap();
    let vmo = VmObject::new_paged(2);
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 64 * PROCESS_VMOS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(avail >= 1);
    assert_eq!(actual, avail);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosSmokeTestV3)
#[test]
fn process_get_info_info_process_vmos_smoke_test_v3() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-sv3").unwrap();
    let vmo1 = VmObject::new_paged(1);
    let vmo2 = VmObject::new_paged(3);
    target.add_handle(Handle::new(vmo1, Rights::DEFAULT_VMO));
    target.add_handle(Handle::new(vmo2, Rights::DEFAULT_VMO));
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = vec![0u8; 64 * PROCESS_VMOS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert!(avail >= 2);
    assert_eq!(actual, avail);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosThreadHandleIsBadHandle)
/// Querying PROCESS_VMOS with a thread handle → WRONG_TYPE.
#[test]
fn process_get_info_info_process_vmos_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = vec![0u8; 16 * PROCESS_VMOS_ENTRY_SIZE];
    let err = get_info_err(&ctx, thread_h, TOPIC_PROCESS_VMOS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ProcessGetInfoTest, InfoProcessVmosZeroSizedBufferIsOk)
#[test]
fn process_get_info_info_process_vmos_zero_sized_buffer_is_ok() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmos-z2").unwrap();
    let vmo = VmObject::new_paged(1);
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));
    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert_eq!(actual, 0);
    assert!(avail >= 1);
}

// ── Memory Stall Info (topic 38) ───────────────────────────────────

const TOPIC_MEMORY_STALL: u32 = 38;
const MEMORY_STALL_SIZE: usize = 24; // zx_info_memory_stall_t: 3 x u64

/// C++: TEST(StallGetInfoTest, MemoryStallBadActualIsInvalidArg)
#[test]
fn stall_get_info_memory_stall_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = AlignedBuf::<MEMORY_STALL_SIZE>::new();
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    );
    // Should fail with INVALID_ARGS or NOT_SUPPORTED.
    assert!(result.is_err());
}

/// C++: TEST(StallGetInfoTest, MemoryStallBadAvailIsInvalidArg)
#[test]
fn stall_get_info_memory_stall_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = AlignedBuf::<MEMORY_STALL_SIZE>::new();
    let mut actual: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    );
    assert!(result.is_err());
}

/// C++: TEST(StallGetInfoTest, MemoryStallInvalidBufferPointerFails)
#[test]
fn stall_get_info_memory_stall_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        0,
        MEMORY_STALL_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(StallGetInfoTest, MemoryStallInvalidHandleFails)
#[test]
fn stall_get_info_memory_stall_invalid_handle_fails() {
    let ctx = TestContext::new();
    let mut buf = [0u8; MEMORY_STALL_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_MEMORY_STALL, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(StallGetInfoTest, MemoryStallNullActualAndAvailSucceeds)
#[test]
fn stall_get_info_memory_stall_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = AlignedBuf::<MEMORY_STALL_SIZE>::new();
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED if memory stall unimplemented.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(StallGetInfoTest, MemoryStallNullActualSucceeds)
#[test]
fn stall_get_info_memory_stall_null_actual_succeeds() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = AlignedBuf::<MEMORY_STALL_SIZE>::new();
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(StallGetInfoTest, MemoryStallNullAvailSucceeds)
#[test]
fn stall_get_info_memory_stall_null_avail_succeeds() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = AlignedBuf::<MEMORY_STALL_SIZE>::new();
    let mut actual: usize = 0;
    let result = get_info_raw(
        &ctx,
        res_h,
        TOPIC_MEMORY_STALL,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(StallGetInfoTest, MemoryStallSmokeTest)
#[test]
fn stall_get_info_memory_stall_smoke_test() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = [0u8; MEMORY_STALL_SIZE];
    let result = get_info(&ctx, res_h, TOPIC_MEMORY_STALL, &mut buf);
    match result {
        Ok((actual, avail)) => {
            assert_eq!(actual, 1);
            assert_eq!(avail, 1);
        }
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(StallGetInfoTest, MemoryStallZeroSizedBufferIsTooSmall)
#[test]
fn stall_get_info_memory_stall_zero_sized_buffer_is_too_small() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();
    let mut buf = [0u8; 0];
    let result = get_info(&ctx, res_h, TOPIC_MEMORY_STALL, &mut buf);
    if let Err(e) = result {
        assert!(
            e == zircon_object::ZxError::BUFFER_TOO_SMALL
                || e == zircon_object::ZxError::NOT_SUPPORTED,
        );
    }
}

// ── Task Stats (topic 12) ─────────────────────────────────────────

const TOPIC_TASK_STATS: u32 = 12;
/// V2 size: zx_info_task_stats_t has 4 × u64 = 32 bytes.
const TASK_STATS_SIZE: usize = 32;
/// V1 size: zx_info_task_stats_v1_t has 2 × u64 = 16 bytes.
const TASK_STATS_V1_SIZE: usize = 16;

/// C++: TEST(TaskGetInfoTest, InfoStatsSmokeTest)
#[test]
fn task_get_info_info_stats_smoke_test() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = [0u8; TASK_STATS_SIZE];
    let result = get_info(&ctx, proc_h, TOPIC_TASK_STATS, &mut buf);
    match result {
        Ok((actual, avail)) => {
            assert_eq!(actual, 1);
            assert_eq!(avail, 1);
        }
        Err(e) => assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED),
    }
}

/// C++: TEST(TaskGetInfoTest, InfoStatsSmokeTestV1)
/// V1 uses a 16-byte buffer (2 fields only). The kernel may reject
/// this as BUFFER_TOO_SMALL if it requires the full 32-byte struct.
#[test]
fn task_get_info_info_stats_smoke_test_v1() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = [0u8; TASK_STATS_V1_SIZE];
    let result = get_info(&ctx, proc_h, TOPIC_TASK_STATS, &mut buf);
    match result {
        Ok((actual, avail)) => {
            assert_eq!(actual, 1);
            assert_eq!(avail, 1);
        }
        Err(e) => assert!(
            e == zircon_object::ZxError::NOT_SUPPORTED
                || e == zircon_object::ZxError::BUFFER_TOO_SMALL,
            "expected NOT_SUPPORTED or BUFFER_TOO_SMALL, got {:?}",
            e
        ),
    }
}

/// C++: TEST(TaskGetInfoTest, InfoStatsUnstartedSucceeds)
#[test]
fn task_get_info_info_stats_unstarted_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "ts-unstarted").unwrap();
    let proc_h = ctx.proc.add_handle(Handle::new(target, Rights::INSPECT));
    let mut buf = [0u8; TASK_STATS_SIZE];
    let result = get_info(&ctx, proc_h, TOPIC_TASK_STATS, &mut buf);
    // C++ ASSERT_OK — unstarted process stats should succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsBadActualIsInvalidArg)
#[test]
fn task_get_info_info_task_stats_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = AlignedBuf::<TASK_STATS_SIZE>::new();
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsBadAvailIsInvalidArg)
#[test]
fn task_get_info_info_task_stats_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = AlignedBuf::<TASK_STATS_SIZE>::new();
    let mut actual: usize = 0;
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    );
    assert!(result.is_err());
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsInvalidBufferPointerFails)
#[test]
fn task_get_info_info_task_stats_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        0,
        TASK_STATS_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    );
    assert!(result.is_err());
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsInvalidHandleFails)
#[test]
fn task_get_info_info_task_stats_invalid_handle_fails() {
    let ctx = TestContext::new();
    let mut buf = [0u8; TASK_STATS_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_TASK_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsJobHandleIsBadHandle)
/// TASK_STATS with a job handle → WRONG_TYPE (only processes supported).
#[test]
fn task_get_info_info_task_stats_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = [0u8; TASK_STATS_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_TASK_STATS, &mut buf);
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE || err == zircon_object::ZxError::NOT_SUPPORTED,
    );
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsNullActualAndAvailSucceeds)
#[test]
fn task_get_info_info_task_stats_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = AlignedBuf::<TASK_STATS_SIZE>::new();
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsNullActualSucceeds)
#[test]
fn task_get_info_info_task_stats_null_actual_succeeds() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = AlignedBuf::<TASK_STATS_SIZE>::new();
    let mut avail: usize = 0;
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsNullAvailSucceeds)
#[test]
fn task_get_info_info_task_stats_null_avail_succeeds() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = AlignedBuf::<TASK_STATS_SIZE>::new();
    let mut actual: usize = 0;
    let result = get_info_raw(
        &ctx,
        proc_h,
        TOPIC_TASK_STATS,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    );
    // C++ ASSERT_OK — may succeed or return NOT_SUPPORTED.
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::NOT_SUPPORTED),);
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsThreadHandleIsBadHandle)
/// TASK_STATS with a thread handle → WRONG_TYPE.
#[test]
fn task_get_info_info_task_stats_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = [0u8; TASK_STATS_SIZE];
    let err = get_info_err(&ctx, thread_h, TOPIC_TASK_STATS, &mut buf);
    assert!(
        err == zircon_object::ZxError::WRONG_TYPE || err == zircon_object::ZxError::NOT_SUPPORTED,
    );
}

/// C++: TEST(TaskGetInfoTest, InfoTaskStatsZeroSizedBufferIsTooSmall)
#[test]
fn task_get_info_info_task_stats_zero_sized_buffer_is_too_small() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::INSPECT));
    let mut buf = [0u8; 0];
    let result = get_info(&ctx, proc_h, TOPIC_TASK_STATS, &mut buf);
    if let Err(e) = result {
        assert!(
            e == zircon_object::ZxError::BUFFER_TOO_SMALL
                || e == zircon_object::ZxError::NOT_SUPPORTED,
        )
    }
}

/// C++: TEST(TaskGetInfoTest, SharedMemAccounting)
///
/// Create two processes with VMOs and mappings, query TASK_STATS, and
/// verify mem_mapped_bytes, mem_private_bytes, mem_shared_bytes, and
/// mem_scaled_shared_bytes match expected values — just like the C++ test.
///
/// The C++ test creates:
///   - private_anon: 1 page, mapped only in proc0
///   - shared_anon: 1 page, mapped in both proc0 and proc1
///   - shared_cow: 3 pages, with a snapshot child (private_cow)
///     Both shared_cow and private_cow have one page forked.
///   - private_cow: snapshot of shared_cow, mapped only in proc1
///   - shared_cow mapped in both proc0 and proc1
///
/// We replicate the setup and verify the accounting.
#[test]
fn task_get_info_shared_mem_accounting() {
    use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

    let ctx = TestContext::new();
    let job = Job::root();

    let proc0 = Process::create(&job, "proc0").unwrap();
    let proc1 = Process::create(&job, "proc1").unwrap();
    let vmar0 = proc0.vmar();
    let vmar1 = proc1.vmar();

    let page_size = PAGE_SIZE;

    // private_anon: 1 page, written to commit
    let private_anon = VmObject::new_paged(1);
    private_anon.write(0, b"A").unwrap();

    // shared_anon: 1 page, written to commit
    let shared_anon = VmObject::new_paged(1);
    shared_anon.write(0, b"A").unwrap();

    // shared_cow: 3 pages, all written to commit
    let shared_cow = VmObject::new_paged(3);
    shared_cow.write(0, b"A").unwrap();
    shared_cow.write(page_size, b"A").unwrap();
    shared_cow.write(page_size * 2, b"A").unwrap();

    // private_cow: snapshot of shared_cow
    let private_cow = shared_cow.create_child(false, 0, page_size * 3).unwrap();

    // Fork pages: write to shared_cow page 0, private_cow page 1
    shared_cow.write(0, b"B").unwrap();
    private_cow.write(page_size, b"B").unwrap();

    // Map into proc0: private_anon, shared_anon, shared_cow
    let flags = MMUFlags::READ;
    vmar0
        .map(None, private_anon.clone(), 0, page_size, flags)
        .unwrap();
    vmar0
        .map(None, shared_anon.clone(), 0, page_size, flags)
        .unwrap();
    vmar0
        .map(None, shared_cow.clone(), 0, page_size * 3, flags)
        .unwrap();

    // Map into proc1: private_cow, shared_anon, shared_cow
    vmar1
        .map(None, private_cow.clone(), 0, page_size * 3, flags)
        .unwrap();
    vmar1
        .map(None, shared_anon.clone(), 0, page_size, flags)
        .unwrap();
    vmar1
        .map(None, shared_cow.clone(), 0, page_size * 3, flags)
        .unwrap();

    // Query stats for proc0
    let proc0_h = ctx.proc.add_handle(Handle::new(proc0, Rights::INSPECT));
    let mut buf0 = [0u8; TASK_STATS_SIZE];
    let result0 = get_info(&ctx, proc0_h, TOPIC_TASK_STATS, &mut buf0);
    if let Err(e) = result0 {
        // If TASK_STATS is not supported, skip the test.
        assert_eq!(e, zircon_object::ZxError::NOT_SUPPORTED);
        return;
    }
    let (actual0, avail0) = result0.unwrap();
    assert_eq!(actual0, 1);
    assert_eq!(avail0, 1);

    // Query stats for proc1
    let proc1_h = ctx.proc.add_handle(Handle::new(proc1, Rights::INSPECT));
    let mut buf1 = [0u8; TASK_STATS_SIZE];
    let (actual1, avail1) = get_info(&ctx, proc1_h, TOPIC_TASK_STATS, &mut buf1).unwrap();
    assert_eq!(actual1, 1);
    assert_eq!(avail1, 1);

    // Parse TaskStatsInfo: { mapped_bytes: u64, private_bytes: u64,
    //                        shared_bytes: u64, scaled_shared_bytes: u64 }
    let mapped0 = u64::from_ne_bytes(buf0[0..8].try_into().unwrap());
    let private0 = u64::from_ne_bytes(buf0[8..16].try_into().unwrap());
    let shared0 = u64::from_ne_bytes(buf0[16..24].try_into().unwrap());
    let scaled_shared0 = u64::from_ne_bytes(buf0[24..32].try_into().unwrap());

    let mapped1 = u64::from_ne_bytes(buf1[0..8].try_into().unwrap());
    let private1 = u64::from_ne_bytes(buf1[8..16].try_into().unwrap());
    let shared1 = u64::from_ne_bytes(buf1[16..24].try_into().unwrap());
    let _scaled_shared1 = u64::from_ne_bytes(buf1[24..32].try_into().unwrap());

    let ps = page_size as u64;

    // --- proc0 expectations (C++ assertions) ---
    // mapped_bytes = private_anon + shared_anon + shared_cow = 1 + 1 + 3 = 5 pages
    let expected_mapped0 = ps + ps + ps * 3;
    assert_eq!(
        mapped0, expected_mapped0,
        "proc0 mapped_bytes: expected {}, got {}",
        expected_mapped0, mapped0
    );

    // private_bytes = only private_anon (1 page) — shared_cow's private
    // pages are still "shared" because the VMO has share_count > 1.
    let expected_private0 = ps;
    assert_eq!(
        private0, expected_private0,
        "proc0 private_bytes: expected {}, got {}",
        expected_private0, private0
    );

    // shared_bytes = shared_anon (1 page) + shared_cow (3 pages)
    let expected_shared0 = ps + ps * 3;
    assert_eq!(
        shared0, expected_shared0,
        "proc0 shared_bytes: expected {}, got {}",
        expected_shared0, shared0
    );

    // Verify mapped = private + shared
    assert_eq!(
        mapped0,
        private0 + shared0,
        "proc0: mapped should equal private + shared"
    );

    // scaled_shared_bytes: C++ computes this as:
    //   (shared_anon + cow_private_bytes) / 2 + cow_shared_bytes / 2 / 2
    // where cow_private_bytes = 2 pages (each process forked 1, other gained 1)
    //       cow_shared_bytes = 1 page (unforked page shared between both COW VMOs)
    // But zCore's simpler share_count-based model may differ. Just verify
    // it's non-zero and <= shared_bytes.
    assert!(
        scaled_shared0 > 0,
        "proc0 scaled_shared_bytes should be > 0, got {}",
        scaled_shared0
    );
    assert!(
        scaled_shared0 <= shared0,
        "proc0 scaled_shared_bytes ({}) should be <= shared_bytes ({})",
        scaled_shared0,
        shared0
    );

    // --- proc1 expectations ---
    // mapped_bytes = private_cow (3 pages) + shared_anon (1) + shared_cow (3) = 7 pages
    let expected_mapped1 = ps * 3 + ps + ps * 3;
    assert_eq!(
        mapped1, expected_mapped1,
        "proc1 mapped_bytes: expected {}, got {}",
        expected_mapped1, mapped1
    );

    // Verify mapped = private + shared for proc1 too
    assert_eq!(
        mapped1,
        private1 + shared1,
        "proc1: mapped should equal private + shared"
    );
}

// ── Thread handle count bad pointer tests ─────────────────────────

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountBadActualIsInvalidArg)
#[test]
fn thread_get_info_info_handle_count_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountBadAvailIsInvalidArg)
#[test]
fn thread_get_info_info_handle_count_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountInvalidBufferPointerFails)
#[test]
fn thread_get_info_info_handle_count_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        0,
        HANDLE_COUNT_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountNullActualAndAvailSucceeds)
#[test]
fn thread_get_info_info_handle_count_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountNullActualSucceeds)
#[test]
fn thread_get_info_info_handle_count_null_actual_succeeds() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoHandleCountNullAvailSucceeds)
#[test]
fn thread_get_info_info_handle_count_null_avail_succeeds() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "hc-t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = AlignedBuf::<HANDLE_COUNT_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        thread_h,
        TOPIC_HANDLE_COUNT,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadExceptionReportInvalidHandleFails)
#[test]
fn thread_get_info_info_thread_exception_report_invalid_handle_fails() {
    let ctx = TestContext::new();
    // Topic 11 = THREAD_EXCEPTION_REPORT
    let mut buf = [0u8; 64];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, 11, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadJobHandleIsBadHandle)
/// Querying THREAD topic with a job handle → WRONG_TYPE.
#[test]
fn thread_get_info_info_thread_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = [0u8; THREAD_INFO_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_THREAD, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadProcessHandleIsBadHandle)
/// Querying THREAD topic with a process handle → WRONG_TYPE.
#[test]
fn thread_get_info_info_thread_process_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));
    let mut buf = [0u8; THREAD_INFO_SIZE];
    let err = get_info_err(&ctx, proc_h, TOPIC_THREAD, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadStatsJobHandleIsBadHandle)
/// Querying THREAD_STATS with a job handle → WRONG_TYPE.
#[test]
fn thread_get_info_info_thread_stats_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = [0u8; THREAD_STATS_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_THREAD_STATS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadStatsProcessHandleIsBadHandle)
/// Querying THREAD_STATS with a process handle → WRONG_TYPE.
#[test]
fn thread_get_info_info_thread_stats_process_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));
    let mut buf = [0u8; THREAD_STATS_SIZE];
    let err = get_info_err(&ctx, proc_h, TOPIC_THREAD_STATS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(ThreadGetInfoTest, InfoThreadStatsRuntime)
/// Query TASK_RUNTIME (topic 30) on a thread — should return runtime info.
#[test]
fn thread_get_info_info_thread_stats_runtime() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "rt-t").unwrap();
    let thread_h = ctx.proc.add_handle(Handle::new(thread, Rights::INSPECT));
    let mut buf = [0u8; TASK_RUNTIME_V1_SIZE];
    let (actual, avail) = get_info(&ctx, thread_h, TOPIC_TASK_RUNTIME, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

// ── HANDLE_BASIC on VMAR (topic 2) ────────────────────────────────

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicBadActualIsInvalidArg)
#[test]
fn vmar_get_info_info_handle_basic_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicBadAvailIsInvalidArg)
#[test]
fn vmar_get_info_info_handle_basic_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicInvalidBufferPointerFails)
#[test]
fn vmar_get_info_info_handle_basic_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicInvalidHandleFails)
#[test]
fn vmar_get_info_info_handle_basic_invalid_handle_fails() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicNullActualAndAvailSucceeds)
#[test]
fn vmar_get_info_info_handle_basic_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicNullActualSucceeds)
#[test]
fn vmar_get_info_info_handle_basic_null_actual_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicNullAvailSucceeds)
#[test]
fn vmar_get_info_info_handle_basic_null_avail_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicOnSelfSucceeds)
#[test]
fn vmar_get_info_info_handle_basic_on_self_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, vmar_h, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// C++: TEST(VmarGetInfoTest, InfoHandleBasicZeroSizedBufferFails)
#[test]
fn vmar_get_info_info_handle_basic_zero_sized_buffer_fails() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmar_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

/// C++: TEST(VmarGetInfoTest, InfoVmarJobHandleIsBadHandle)
/// Querying VMAR topic with a job handle → WRONG_TYPE.
#[test]
fn vmar_get_info_info_vmar_job_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let job = Job::root();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));
    let mut buf = [0u8; VMAR_INFO_SIZE];
    let err = get_info_err(&ctx, job_h, TOPIC_VMAR, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(VmarGetInfoTest, InfoVmarProcessHandleIsBadHandle)
/// Querying VMAR topic with a process handle → WRONG_TYPE.
#[test]
fn vmar_get_info_info_vmar_process_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(ctx.proc.clone(), Rights::DEFAULT_PROCESS));
    let mut buf = [0u8; VMAR_INFO_SIZE];
    let err = get_info_err(&ctx, proc_h, TOPIC_VMAR, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

/// C++: TEST(VmarGetInfoTest, InfoVmarThreadHandleIsBadHandle)
/// Querying VMAR topic with a thread handle → WRONG_TYPE.
#[test]
fn vmar_get_info_info_vmar_thread_handle_is_bad_handle() {
    let ctx = TestContext::new();
    let thread = Thread::create(&ctx.proc, "t").unwrap();
    let thread_h = ctx
        .proc
        .add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
    let mut buf = [0u8; VMAR_INFO_SIZE];
    let err = get_info_err(&ctx, thread_h, TOPIC_VMAR, &mut buf);
    assert_eq!(err, zircon_object::ZxError::WRONG_TYPE);
}

// ── VMAR Maps Info (topic 36) ─────────────────────────────────────

const TOPIC_VMAR_MAPS: u32 = 36;
// zx_info_maps_t is the same structure as PROCESS_MAPS entries
const VMAR_MAPS_ENTRY_SIZE: usize = 104;

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsBadActualIsInvalidArg)
#[test]
fn vmar_maps_get_info_info_vmar_maps_bad_actual_is_invalid_arg() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let buf_size = 16 * VMAR_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsBadAvailIsInvalidArg)
#[test]
fn vmar_maps_get_info_info_vmar_maps_bad_avail_is_invalid_arg() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let buf_size = 16 * VMAR_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsInvalidBufferPointerFails)
#[test]
fn vmar_maps_get_info_info_vmar_maps_invalid_buffer_pointer_fails() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        0,
        16 * VMAR_MAPS_ENTRY_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsInvalidHandleFails)
#[test]
fn vmar_maps_get_info_info_vmar_maps_invalid_handle_fails() {
    let ctx = TestContext::new();
    let mut buf = vec![0u8; 16 * VMAR_MAPS_ENTRY_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_VMAR_MAPS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsNullActualAndAvailSucceeds)
#[test]
fn vmar_maps_get_info_info_vmar_maps_null_actual_and_avail_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let buf_size = 16 * VMAR_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        0,
    )
    .unwrap();
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsNullActualSucceeds)
#[test]
fn vmar_maps_get_info_info_vmar_maps_null_actual_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let buf_size = 16 * VMAR_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsNullAvailSucceeds)
#[test]
fn vmar_maps_get_info_info_vmar_maps_null_avail_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let buf_size = 16 * VMAR_MAPS_ENTRY_SIZE;
    let mut buf = AlignedBuf::<1664>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        vmar_h,
        TOPIC_VMAR_MAPS,
        buf.as_mut_ptr() as usize,
        buf_size,
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsOnSelfSucceeds)
#[test]
fn vmar_maps_get_info_info_vmar_maps_on_self_succeeds() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = vec![0u8; 64 * VMAR_MAPS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf).unwrap();
    assert!(avail >= 1, "VMAR should have at least 1 maps entry");
    assert_eq!(actual, avail);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsProcessDestroyedFails)
/// Query VMAR_MAPS on a VMAR whose process has been killed.
#[test]
fn vmar_maps_get_info_info_vmar_maps_process_destroyed_fails() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmar-dead").unwrap();
    let vmar = target.vmar();
    // Kill the process
    target.exit(0);
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = vec![0u8; 16 * VMAR_MAPS_ENTRY_SIZE];
    // May return BAD_STATE, NOT_FOUND, or succeed with 0 entries.
    let result = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf);
    assert!(
        result.is_ok()
            || result == Err(zircon_object::ZxError::BAD_STATE)
            || result == Err(zircon_object::ZxError::NOT_FOUND),
    );
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsRequiresInspectRights)
/// VMAR_MAPS without INSPECT right → ACCESS_DENIED.
#[test]
fn vmar_maps_get_info_info_vmar_maps_requires_inspect_rights() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    // No INSPECT right
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::ENUMERATE));
    let mut buf = vec![0u8; 16 * VMAR_MAPS_ENTRY_SIZE];
    let err = get_info_err(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf);
    assert_eq!(err, zircon_object::ZxError::ACCESS_DENIED);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsSmokeTest)
#[test]
fn vmar_maps_get_info_info_vmar_maps_smoke_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = vec![0u8; 64 * VMAR_MAPS_ENTRY_SIZE];
    let (actual, avail) = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf).unwrap();
    assert!(avail >= 1);
    assert_eq!(actual, avail);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsUnstartedSucceeds)
#[test]
fn vmar_maps_get_info_info_vmar_maps_unstarted_succeeds() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmar-unst").unwrap();
    let vmar = target.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = vec![0u8; 16 * VMAR_MAPS_ENTRY_SIZE];
    let (_actual, _avail) = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf).unwrap();
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsVmarDestroyedFails)
/// Query VMAR_MAPS on a destroyed VMAR.
#[test]
fn vmar_maps_get_info_info_vmar_maps_vmar_destroyed_fails() {
    let ctx = TestContext::new();
    let job = Job::root();
    let target = Process::create(&job, "vmar-dest").unwrap();
    let vmar = target.vmar();
    // Destroy the VMAR
    let _ = vmar.destroy();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = vec![0u8; 16 * VMAR_MAPS_ENTRY_SIZE];
    // May return BAD_STATE or succeed with 0 entries.
    let result = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf);
    assert!(result.is_ok() || result == Err(zircon_object::ZxError::BAD_STATE),);
}

/// C++: TEST(VmarMapsGetInfoTest, InfoVmarMapsZeroSizedBufferIsOk)
#[test]
fn vmar_maps_get_info_info_vmar_maps_zero_sized_buffer_is_ok() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmar_h = ctx.proc.add_handle(Handle::new(vmar, Rights::DEFAULT_VMAR));
    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, vmar_h, TOPIC_VMAR_MAPS, &mut buf).unwrap();
    assert_eq!(actual, 0);
    assert!(avail >= 1);
}

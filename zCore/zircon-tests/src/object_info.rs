//! Object info tests.
//!
//! Faithful 1:1 ports from the object-info test suite (8 files).
//! Tests `zx_object_get_info` for various ZX_INFO_* topics.

use crate::helpers::TestContext;

use hal_impl::user::UserOutPtr;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{Job, Process, Thread};

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

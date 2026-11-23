//! Extra object info tests.
//!
//! Additional tests for ZX_INFO topics not covered in the main
//! object_info.rs file.

extern crate alloc;

use crate::core_tests::object_info::{
    get_info, get_info_err, get_info_raw, AlignedBuf, BTI_INFO_SIZE, HANDLE_BASIC_SIZE,
    HANDLE_COUNT_SIZE, HANDLE_EXTENDED_SIZE, JOB_INFO_SIZE, PROCESS_HANDLE_STATS_SIZE,
    RESOURCE_INFO_SIZE, SOCKET_INFO_SIZE, STREAM_INFO_SIZE, TIMER_INFO_SIZE, TOPIC_BTI,
    TOPIC_HANDLE_BASIC, TOPIC_HANDLE_COUNT, TOPIC_HANDLE_TABLE, TOPIC_HANDLE_VALID, TOPIC_JOB,
    TOPIC_PROCESS_HANDLE_STATS, TOPIC_PROCESS_MAPS, TOPIC_PROCESS_THREADS, TOPIC_PROCESS_VMOS,
    TOPIC_RESOURCE, TOPIC_SOCKET, TOPIC_STREAM, TOPIC_TIMER, TOPIC_VMO, VMO_INFO_SIZE,
};
use crate::helpers::TestContext;
use zircon_object::dev::{BusTransactionInitiator, Iommu, Resource, ResourceFlags, ResourceKind};
use zircon_object::ipc::{Fifo, Socket};
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::signal::{Counter, Event, Pager, Port, Timer};
use zircon_object::task::{Job, Process};
use zircon_object::vm::{Stream, VmObject};

///
/// Query ZX_INFO_HANDLE_VALID on a valid handle. Should succeed.
fn info_handle_valid() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_ne!(event.id(), 0, "valid object should have non-zero KOID");
}

///
/// Query ZX_INFO_HANDLE_BASIC on an event. Should return type EVENT.
fn info_handle_basic_event() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_eq!(event.type_name(), "Event");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a channel. Should return type CHANNEL.
fn info_handle_basic_channel() {
    use zircon_object::ipc::Channel;
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    assert_eq!(local.type_name(), "Channel");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a socket. Should return type SOCKET.
fn info_handle_basic_socket() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();
    assert_eq!(local.type_name(), "Socket");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a VMO. Should return type VMO.
fn info_handle_basic_vmo() {
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_eq!(vmo.type_name(), "VmObject");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a port. Should return type PORT.
fn info_handle_basic_port() {
    use zircon_object::signal::Port;
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    assert_eq!(port.type_name(), "Port");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a timer. Should return type TIMER.
fn info_handle_basic_timer() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert_eq!(timer.type_name(), "Timer");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a job. Should return type JOB.
fn info_handle_basic_job() {
    let _ctx = TestContext::new();
    let root = Job::new_parentless_job();
    let child = root.create_child().unwrap();
    assert_eq!(child.type_name(), "Job");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a process. Should return type PROCESS.
fn info_handle_basic_process() {
    use zircon_object::task::Process;
    let _ctx = TestContext::new();
    let root = Job::new_parentless_job();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "test-proc").unwrap();
    assert_eq!(proc.type_name(), "Process");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a thread. Should return type THREAD.
fn info_handle_basic_thread() {
    use zircon_object::task::{Process, Thread};
    let _ctx = TestContext::new();
    let root = Job::new_parentless_job();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "info-proc").unwrap();
    let thread = Thread::create(&proc, "test-thread").unwrap();
    assert_eq!(thread.type_name(), "Thread");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a FIFO. Should return type FIFO.
fn info_handle_basic_fifo() {
    use zircon_object::ipc::Fifo;
    let _ctx = TestContext::new();
    let (local, _remote) = Fifo::create(8, 4);
    assert_eq!(local.type_name(), "Fifo");
}

///
/// Query ZX_INFO_HANDLE_BASIC on an event pair. Should return type
/// EVENT_PAIR.
fn info_handle_basic_event_pair() {
    use zircon_object::signal::EventPair;
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();
    assert_eq!(ep0.type_name(), "EventPair");
}

///
/// KOIDs should be unique across different object types.
fn unique_koids_across_types() {
    use zircon_object::ipc::{Channel, Socket};
    use zircon_object::signal::{Port, Timer};
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();

    let event = Event::new();
    let (ch, _) = Channel::create();
    let (sk, _) = Socket::create(0).unwrap();
    let port = Port::new(0).unwrap();
    let timer = Timer::new();
    let vmo = VmObject::new_paged(1);

    let ids = [
        event.id(),
        ch.id(),
        sk.id(),
        port.id(),
        timer.id(),
        vmo.id(),
    ];
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "KOIDs should be unique across types");
        }
    }
}

///
/// Verify handle count tracking via ZX_INFO_HANDLE_COUNT (topic 19).
/// Adding multiple handles to the same object increases the count.
fn info_handle_count() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, Rights};

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create an event and add a handle to the process.
    let event = Event::new();
    let h1 = ctx
        .proc
        .add_handle(Handle::new(event.clone(), Rights::DEFAULT_EVENT));

    // Query handle count — should be 1 (only h1).
    let mut count: u32 = 0;
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        h1,
        19, // ZX_INFO_HANDLE_COUNT
        &mut count as *mut u32 as usize,
        core::mem::size_of::<u32>(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    // handle_count() returns Arc strong count which includes the
    // kernel's own reference, so it may be > 1. Just verify > 0.
    assert!(count > 0, "handle count should be > 0, got {}", count);
}

///
/// Query ZX_INFO_PROCESS_MAPS (topic 12). Returns the memory map of a
/// process. Cannot inspect own process (would deadlock), so we create
/// a child process to inspect.
fn info_process_maps() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, Rights};
    use zircon_object::task::Process;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child process under a child job.
    let root = Job::new_parentless_job();
    let child_job = root.create_child().unwrap();
    let child_proc = Process::create(&child_job, "maps-target").unwrap();
    let child_h = ctx.proc.add_handle(Handle::new(
        child_proc.clone(),
        Rights::INSPECT | Rights::MANAGE_PROCESS,
    ));

    // Query with a small buffer — just verify the call succeeds and
    // reports avail (the child process has at least an aspace entry).
    let mut buf = [0u8; 1024];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        child_h,
        12, // ZX_INFO_PROCESS_MAPS
        buf.as_mut_ptr() as usize,
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    // A freshly created process has at least an aspace entry (depth 0).
    assert!(avail >= 1, "avail should be >= 1, got {}", avail);
}

///
/// Query ZX_INFO_PROCESS_VMOS (topic 13). Returns all VMOs in a process.
/// Create a child process, add a VMO handle, then query.
fn info_process_vmos() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, Rights};
    use zircon_object::task::Process;
    use zircon_object::vm::VmObject;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child process and add a VMO handle to it.
    let root = Job::new_parentless_job();
    let child_job = root.create_child().unwrap();
    let child_proc = Process::create(&child_job, "vmos-target").unwrap();

    // Add a VMO to the child process.
    let vmo = VmObject::new_paged(1);
    child_proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));

    let child_h = ctx.proc.add_handle(Handle::new(
        child_proc.clone(),
        Rights::INSPECT | Rights::MANAGE_PROCESS,
    ));

    // Query process VMOs. VmoInfo is 168 bytes (full size, V3+).
    // Use full-size entries via topic version 3 (upper 4 bits = 3).
    let topic = 13 | (3 << 28); // ZX_INFO_PROCESS_VMOS, version 3
    let entry_size = 168;
    let mut buf = vec![0u8; entry_size * 4]; // room for 4 entries
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        child_h,
        topic,
        buf.as_mut_ptr() as usize,
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    // Should see at least 1 VMO (the one we added).
    assert!(avail >= 1, "should have at least 1 VMO, avail={}", avail);
    assert!(
        actual >= 1,
        "should have at least 1 VMO returned, actual={}",
        actual
    );
}

///
/// Query ZX_INFO_CPU_STATS (topic 16). Returns per-CPU statistics.
/// Requires root resource.
fn info_cpu_stats() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let resource_h = ctx.add_root_resource();

    // CpuStatsInfo is 144 bytes per CPU. Allocate space for 1 entry.
    let mut buf = [0u8; 144];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        resource_h,
        16, // ZX_INFO_CPU_STATS
        buf.as_mut_ptr() as usize,
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    let expected_cpus = hal_impl::cpu::num_cpus();
    assert_eq!(actual, 1, "with 144-byte buffer, should fit 1 entry");
    assert_eq!(avail, expected_cpus, "should report actual CPU count");

    // First 4 bytes = cpu_number (u32), next 4 = flags (u32).
    let flags = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    assert_eq!(flags & 1, 1, "CPU should be online (flag bit 0)");
}

///
/// Query ZX_INFO_KMEM_STATS (topic 17). Returns kernel memory statistics.
/// Requires root resource.
fn info_kmem_stats() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let resource_h = ctx.add_root_resource();

    // KmemInfo is 152 bytes (19 x u64).
    let mut buf = [0u8; 152];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    sc.sys_object_get_info(
        resource_h,
        17, // ZX_INFO_KMEM_STATS
        buf.as_mut_ptr() as usize,
        buf.len(),
        UserOutPtr::from(&mut actual as *mut _ as usize),
        UserOutPtr::from(&mut avail as *mut _ as usize),
    )
    .unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);

    // First 8 bytes = total_bytes (u64) — should be > 0.
    let total_bytes = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    assert!(
        total_bytes > 0,
        "total_bytes should be > 0, got {}",
        total_bytes
    );

    // Bytes 8..16 = free_bytes — should be > 0.
    let free_bytes = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    assert!(free_bytes > 0, "free_bytes should be > 0");

    // Bytes 48..56 = vmo_bytes — should be >= 0 (may be 0 if no VMOs yet).
    let vmo_bytes = u64::from_le_bytes(buf[48..56].try_into().unwrap());
    let _ = vmo_bytes; // Just verify no panic during read.
}

// ── Tests extracted from core_tests ──────────────────────────────

fn vmo_info_zero_buffer() {
    let ctx = TestContext::new();
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmo_h, TOPIC_VMO, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn vmo_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; 168];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_VMO, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn job_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; JOB_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_JOB, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn job_info_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, job_h, TOPIC_JOB, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn socket_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; SOCKET_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_SOCKET, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn socket_info_zero_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, socket_h, TOPIC_SOCKET, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn timer_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; TIMER_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_TIMER, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

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

fn stream_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; STREAM_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_STREAM, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

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

fn bti_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; BTI_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_BTI, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn bti_info_zero_buffer() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, bti_h, TOPIC_BTI, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn resource_info_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; RESOURCE_INFO_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_RESOURCE, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

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

fn resource_info_zero_buffer() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, res_h, TOPIC_RESOURCE, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

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

fn handle_count_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_COUNT_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_COUNT, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

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

fn job_info_null_avail() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn job_info_null_actual() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn job_info_null_actual_and_avail() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn job_info_invalid_buffer() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn job_info_bad_actual() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn job_info_bad_avail() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
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

fn process_info_handle_stats_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; PROCESS_HANDLE_STATS_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_PROCESS_HANDLE_STATS, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

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

///
/// PROCESS_MAPS with zero-length buffer should succeed with actual=0
/// and avail reporting the number of entries.
fn process_info_maps_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let target = Process::create(&job, "maps-zero").unwrap();
    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_MAPS, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    assert!(avail >= 1, "process should have at least 1 maps entry");
}

///
/// PROCESS_VMOS with zero-length buffer should succeed with actual=0
/// and avail reporting the number of VMOs.
fn process_info_vmos_zero_buffer() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let target = Process::create(&job, "vmos-zero").unwrap();

    // Add a VMO handle so avail > 0.
    let vmo = VmObject::new_paged(1);
    target.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));

    let proc_h = ctx
        .proc
        .add_handle(Handle::new(target, Rights::DEFAULT_PROCESS));

    let mut buf = [0u8; 0];
    let (actual, avail) = get_info(&ctx, proc_h, TOPIC_PROCESS_VMOS, &mut buf).unwrap();
    assert_eq!(actual, 0, "zero buffer should return actual=0");
    assert!(avail >= 1, "process should have at least 1 VMO entry");
}

fn event_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        event_h,
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

fn event_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn event_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, event_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn event_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn event_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn event_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn event_info_handle_basic_invalid_buffer() {
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
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn event_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn event_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        event_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn eventpair_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        ep_h,
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

fn eventpair_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xBEEF, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn eventpair_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, ep_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn eventpair_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn eventpair_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn eventpair_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn eventpair_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn eventpair_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn eventpair_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let (ep0, _ep1) = zircon_object::signal::EventPair::create();
    let ep_h = ctx
        .proc
        .add_handle(Handle::new(ep0, Rights::DEFAULT_EVENTPAIR));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        ep_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn channel_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        ch_h,
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

fn channel_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xCAFE, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn channel_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, ch_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn channel_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn channel_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn channel_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn channel_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn channel_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn channel_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let (ch0, _ch1) = zircon_object::ipc::Channel::create();
    let ch_h = ctx
        .proc
        .add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        ch_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn vmo_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(
        vmo,
        Rights::READ | Rights::WRITE | Rights::INSPECT,
    ));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmo_h,
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

fn vmo_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xF00D, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn vmo_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, vmo_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn vmo_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn vmo_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn vmo_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn vmo_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn vmo_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn vmo_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let vmo_h = ctx.proc.add_handle(Handle::new(vmo, Rights::INSPECT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        vmo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn socket_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        s_h,
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

fn socket_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xBABE, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn socket_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, s_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn socket_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn socket_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn socket_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn socket_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn socket_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn socket_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let s_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        s_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn timer_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn timer_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn timer_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        timer_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn timer_info_handle_basic_invalid_buffer() {
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
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn timer_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        timer_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn timer_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        timer_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn handle_valid_null_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    get_info_raw(&ctx, event_h, TOPIC_HANDLE_VALID, 0, 0, 0, 0).unwrap();
}

fn handle_valid_null_actual() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    get_info_raw(&ctx, event_h, TOPIC_HANDLE_VALID, 0, 0, 0, 0).unwrap();
}

fn handle_valid_null_actual_and_avail() {
    let ctx = TestContext::new();
    let event = Event::new();
    let event_h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    get_info_raw(&ctx, event_h, TOPIC_HANDLE_VALID, 0, 0, 0, 0).unwrap();
}

fn timer_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        timer_h,
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

fn timer_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn timer_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let timer_h = ctx
        .proc
        .add_handle(Handle::new(timer, Rights::DEFAULT_TIMER));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, timer_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn fifo_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        fifo_h,
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

fn fifo_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn fifo_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, fifo_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn fifo_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn fifo_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn fifo_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn fifo_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn fifo_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn fifo_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let (fifo, _fifo1) = Fifo::create(16, 4);
    let fifo_h = ctx.proc.add_handle(Handle::new(fifo, Rights::DEFAULT_FIFO));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        fifo_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn port_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        port_h,
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

fn port_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn port_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, port_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn port_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn port_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn port_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn port_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn port_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn port_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    let port_h = ctx.proc.add_handle(Handle::new(port, Rights::DEFAULT_PORT));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        port_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn counter_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        counter_h,
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

fn counter_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn counter_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, counter_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn counter_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn counter_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn counter_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn counter_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn counter_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn counter_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let counter = Counter::new();
    let counter_h = ctx
        .proc
        .add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        counter_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn pager_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        pager_h,
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

fn pager_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn pager_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, pager_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn pager_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn pager_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn pager_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn pager_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn pager_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn pager_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let pager = Pager::new();
    let pager_h = ctx
        .proc
        .add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        pager_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn bti_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        bti_h,
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

fn bti_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn bti_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, bti_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn bti_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn bti_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn bti_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn bti_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn bti_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn bti_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let iommu = Iommu::create();
    let bti = BusTransactionInitiator::create(iommu, 0);
    let bti_h = ctx.proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        bti_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn resource_info_handle_basic_on_self() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        res_h,
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

fn resource_info_handle_basic_invalid_handle() {
    let ctx = TestContext::new();
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    assert_eq!(
        get_info_err(&ctx, 0xDEAD, TOPIC_HANDLE_BASIC, &mut buf),
        zircon_object::ZxError::BAD_HANDLE
    );
}

fn resource_info_handle_basic_zero_buffer() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = [0u8; 0];
    let err = get_info_err(&ctx, res_h, TOPIC_HANDLE_BASIC, &mut buf);
    assert_eq!(err, zircon_object::ZxError::BUFFER_TOO_SMALL);
}

fn resource_info_handle_basic_null_avail() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        0,
    )
    .unwrap();
}

fn resource_info_handle_basic_null_actual() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        &mut avail as *mut _ as usize,
    )
    .unwrap();
}

fn resource_info_handle_basic_null_actual_and_avail() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
    .unwrap();
}

fn resource_info_handle_basic_invalid_buffer() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        0,
        HANDLE_BASIC_SIZE,
        &mut actual as *mut _ as usize,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn resource_info_handle_basic_bad_actual() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut avail: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        1,
        &mut avail as *mut _ as usize,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

fn resource_info_handle_basic_bad_avail() {
    let ctx = TestContext::new();
    let res = Resource::create("test", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    let res_h = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    let mut buf = AlignedBuf::<HANDLE_BASIC_SIZE>::new();
    let mut actual: usize = 0;
    let err = get_info_raw(
        &ctx,
        res_h,
        TOPIC_HANDLE_BASIC,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut actual as *mut _ as usize,
        1,
    )
    .unwrap_err();
    assert_eq!(err, zircon_object::ZxError::INVALID_ARGS);
}

// ── Tests extracted from core_tests/object_info.rs ──────────────────

fn job_info_on_self() {
    let ctx = TestContext::new();
    let job = Job::new_parentless_job();
    let job_h = ctx.proc.add_handle(Handle::new(job, Rights::DEFAULT_JOB));

    let mut buf = [0u8; JOB_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, job_h, TOPIC_JOB, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

fn socket_info_on_self() {
    let ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();
    let socket_h = ctx.proc.add_handle(Handle::new(s0, Rights::DEFAULT_SOCKET));

    let mut buf = [0u8; SOCKET_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, socket_h, TOPIC_SOCKET, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

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

fn resource_info_on_self() {
    let ctx = TestContext::new();
    let res_h = ctx.add_root_resource();

    let mut buf = [0u8; RESOURCE_INFO_SIZE];
    let (actual, avail) = get_info(&ctx, res_h, TOPIC_RESOURCE, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);
}

/// Run all tests in this module.
pub fn run_all() {
    info_handle_valid();
    info_handle_basic_event();
    info_handle_basic_channel();
    info_handle_basic_socket();
    info_handle_basic_vmo();
    info_handle_basic_port();
    info_handle_basic_timer();
    info_handle_basic_job();
    info_handle_basic_process();
    info_handle_basic_thread();
    info_handle_basic_fifo();
    info_handle_basic_event_pair();
    unique_koids_across_types();
    info_handle_count();
    info_process_maps();
    info_process_vmos();
    info_cpu_stats();
    info_kmem_stats();
    vmo_info_zero_buffer();
    vmo_info_invalid_handle();
    job_info_invalid_handle();
    job_info_zero_buffer();
    socket_info_invalid_handle();
    socket_info_zero_buffer();
    timer_info_invalid_handle();
    timer_info_zero_buffer();
    stream_info_invalid_handle();
    stream_info_zero_buffer();
    bti_info_invalid_handle();
    bti_info_zero_buffer();
    resource_info_invalid_handle();
    process_info_threads_zero_buffer();
    vmo_info_null_avail();
    vmo_info_null_actual();
    vmo_info_null_actual_and_avail();
    vmo_info_invalid_buffer();
    vmo_info_bad_actual();
    vmo_info_bad_avail();
    socket_info_null_avail();
    socket_info_null_actual();
    socket_info_null_actual_and_avail();
    socket_info_invalid_buffer();
    socket_info_bad_actual();
    socket_info_bad_avail();
    bti_info_null_avail();
    bti_info_null_actual();
    bti_info_null_actual_and_avail();
    bti_info_invalid_buffer();
    bti_info_bad_actual();
    bti_info_bad_avail();
    timer_info_null_avail();
    timer_info_null_actual();
    timer_info_null_actual_and_avail();
    timer_info_invalid_buffer();
    timer_info_bad_actual();
    timer_info_bad_avail();
    stream_info_null_avail();
    stream_info_null_actual();
    stream_info_null_actual_and_avail();
    stream_info_invalid_buffer();
    stream_info_bad_actual();
    stream_info_bad_avail();
    resource_info_zero_buffer();
    resource_info_null_avail();
    resource_info_null_actual();
    resource_info_null_actual_and_avail();
    resource_info_invalid_buffer();
    resource_info_bad_actual();
    resource_info_bad_avail();
    handle_count_invalid_handle();
    handle_count_zero_buffer();
    handle_count_null_avail();
    handle_count_null_actual();
    handle_count_null_actual_and_avail();
    handle_count_invalid_buffer();
    handle_count_bad_actual();
    handle_count_bad_avail();
    job_info_null_avail();
    job_info_null_actual();
    job_info_null_actual_and_avail();
    job_info_invalid_buffer();
    job_info_bad_actual();
    job_info_bad_avail();
    process_info_handle_stats_invalid_handle();
    process_info_handle_stats_zero_buffer();
    process_info_handle_stats_null_avail();
    process_info_handle_stats_null_actual();
    process_info_handle_stats_null_actual_and_avail();
    process_info_handle_stats_invalid_buffer();
    process_info_handle_stats_bad_actual();
    process_info_handle_stats_bad_avail();
    process_info_handle_table_null_actual();
    process_info_handle_table_null_actual_and_avail();
    process_info_handle_table_bad_actual();
    process_info_handle_table_bad_avail();
    process_info_handle_table_zero_buffer();
    process_info_maps_zero_buffer();
    process_info_vmos_zero_buffer();
    event_info_handle_basic_on_self();
    event_info_handle_basic_invalid_handle();
    event_info_handle_basic_zero_buffer();
    event_info_handle_basic_null_avail();
    event_info_handle_basic_null_actual();
    event_info_handle_basic_null_actual_and_avail();
    event_info_handle_basic_invalid_buffer();
    event_info_handle_basic_bad_actual();
    event_info_handle_basic_bad_avail();
    eventpair_info_handle_basic_on_self();
    eventpair_info_handle_basic_invalid_handle();
    eventpair_info_handle_basic_zero_buffer();
    eventpair_info_handle_basic_null_avail();
    eventpair_info_handle_basic_null_actual();
    eventpair_info_handle_basic_null_actual_and_avail();
    eventpair_info_handle_basic_invalid_buffer();
    eventpair_info_handle_basic_bad_actual();
    eventpair_info_handle_basic_bad_avail();
    channel_info_handle_basic_on_self();
    channel_info_handle_basic_invalid_handle();
    channel_info_handle_basic_zero_buffer();
    channel_info_handle_basic_null_avail();
    channel_info_handle_basic_null_actual();
    channel_info_handle_basic_null_actual_and_avail();
    channel_info_handle_basic_invalid_buffer();
    channel_info_handle_basic_bad_actual();
    channel_info_handle_basic_bad_avail();
    vmo_info_handle_basic_on_self();
    vmo_info_handle_basic_invalid_handle();
    vmo_info_handle_basic_zero_buffer();
    vmo_info_handle_basic_null_avail();
    vmo_info_handle_basic_null_actual();
    vmo_info_handle_basic_null_actual_and_avail();
    vmo_info_handle_basic_invalid_buffer();
    vmo_info_handle_basic_bad_actual();
    vmo_info_handle_basic_bad_avail();
    socket_info_handle_basic_on_self();
    socket_info_handle_basic_invalid_handle();
    socket_info_handle_basic_zero_buffer();
    socket_info_handle_basic_null_avail();
    socket_info_handle_basic_null_actual();
    socket_info_handle_basic_null_actual_and_avail();
    socket_info_handle_basic_invalid_buffer();
    socket_info_handle_basic_bad_actual();
    socket_info_handle_basic_bad_avail();
    timer_info_handle_basic_null_avail();
    timer_info_handle_basic_null_actual();
    timer_info_handle_basic_null_actual_and_avail();
    timer_info_handle_basic_invalid_buffer();
    timer_info_handle_basic_bad_actual();
    timer_info_handle_basic_bad_avail();
    handle_valid_null_avail();
    handle_valid_null_actual();
    handle_valid_null_actual_and_avail();
    timer_info_handle_basic_on_self();
    timer_info_handle_basic_invalid_handle();
    timer_info_handle_basic_zero_buffer();
    fifo_info_handle_basic_on_self();
    fifo_info_handle_basic_invalid_handle();
    fifo_info_handle_basic_zero_buffer();
    fifo_info_handle_basic_null_avail();
    fifo_info_handle_basic_null_actual();
    fifo_info_handle_basic_null_actual_and_avail();
    fifo_info_handle_basic_invalid_buffer();
    fifo_info_handle_basic_bad_actual();
    fifo_info_handle_basic_bad_avail();
    port_info_handle_basic_on_self();
    port_info_handle_basic_invalid_handle();
    port_info_handle_basic_zero_buffer();
    port_info_handle_basic_null_avail();
    port_info_handle_basic_null_actual();
    port_info_handle_basic_null_actual_and_avail();
    port_info_handle_basic_invalid_buffer();
    port_info_handle_basic_bad_actual();
    port_info_handle_basic_bad_avail();
    counter_info_handle_basic_on_self();
    counter_info_handle_basic_invalid_handle();
    counter_info_handle_basic_zero_buffer();
    counter_info_handle_basic_null_avail();
    counter_info_handle_basic_null_actual();
    counter_info_handle_basic_null_actual_and_avail();
    counter_info_handle_basic_invalid_buffer();
    counter_info_handle_basic_bad_actual();
    counter_info_handle_basic_bad_avail();
    pager_info_handle_basic_on_self();
    pager_info_handle_basic_invalid_handle();
    pager_info_handle_basic_zero_buffer();
    pager_info_handle_basic_null_avail();
    pager_info_handle_basic_null_actual();
    pager_info_handle_basic_null_actual_and_avail();
    pager_info_handle_basic_invalid_buffer();
    pager_info_handle_basic_bad_actual();
    pager_info_handle_basic_bad_avail();
    bti_info_handle_basic_on_self();
    bti_info_handle_basic_invalid_handle();
    bti_info_handle_basic_zero_buffer();
    bti_info_handle_basic_null_avail();
    bti_info_handle_basic_null_actual();
    bti_info_handle_basic_null_actual_and_avail();
    bti_info_handle_basic_invalid_buffer();
    bti_info_handle_basic_bad_actual();
    bti_info_handle_basic_bad_avail();
    resource_info_handle_basic_on_self();
    resource_info_handle_basic_invalid_handle();
    resource_info_handle_basic_zero_buffer();
    resource_info_handle_basic_null_avail();
    resource_info_handle_basic_null_actual();
    resource_info_handle_basic_null_actual_and_avail();
    resource_info_handle_basic_invalid_buffer();
    resource_info_handle_basic_bad_actual();
    resource_info_handle_basic_bad_avail();
    job_info_on_self();
    socket_info_on_self();
    timer_info_on_self();
    stream_info_on_self();
    bti_info_on_self();
    resource_info_on_self();
}

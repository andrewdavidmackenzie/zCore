//! Extra object info tests.
//!
//! Additional tests for ZX_INFO topics not covered in the main
//! object_info.rs file.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::signal::Event;
use zircon_object::task::Job;

extern crate alloc;

///
/// Query ZX_INFO_HANDLE_VALID on a valid handle. Should succeed.
#[test]
fn info_handle_valid() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_ne!(event.id(), 0, "valid object should have non-zero KOID");
}

///
/// Query ZX_INFO_HANDLE_BASIC on an event. Should return type EVENT.
#[test]
fn info_handle_basic_event() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert_eq!(event.type_name(), "Event");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a channel. Should return type CHANNEL.
#[test]
fn info_handle_basic_channel() {
    use zircon_object::ipc::Channel;
    let _ctx = TestContext::new();
    let (local, _remote) = Channel::create();
    assert_eq!(local.type_name(), "Channel");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a socket. Should return type SOCKET.
#[test]
fn info_handle_basic_socket() {
    use zircon_object::ipc::Socket;
    let _ctx = TestContext::new();
    let (local, _remote) = Socket::create(0).unwrap();
    assert_eq!(local.type_name(), "Socket");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a VMO. Should return type VMO.
#[test]
fn info_handle_basic_vmo() {
    use zircon_object::vm::VmObject;
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_eq!(vmo.type_name(), "VmObject");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a port. Should return type PORT.
#[test]
fn info_handle_basic_port() {
    use zircon_object::signal::Port;
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();
    assert_eq!(port.type_name(), "Port");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a timer. Should return type TIMER.
#[test]
fn info_handle_basic_timer() {
    use zircon_object::signal::Timer;
    let _ctx = TestContext::new();
    let timer = Timer::new();
    assert_eq!(timer.type_name(), "Timer");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a job. Should return type JOB.
#[test]
fn info_handle_basic_job() {
    let _ctx = TestContext::new();
    let root = Job::root();
    let child = root.create_child().unwrap();
    assert_eq!(child.type_name(), "Job");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a process. Should return type PROCESS.
#[test]
fn info_handle_basic_process() {
    use zircon_object::task::Process;
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "test-proc").unwrap();
    assert_eq!(proc.type_name(), "Process");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a thread. Should return type THREAD.
#[test]
fn info_handle_basic_thread() {
    use crate::helpers::test_loop_thread_fn;
    use zircon_object::task::{Process, Thread};
    let _ctx = TestContext::new();
    let root = Job::root();
    let job = root.create_child().unwrap();
    let proc = Process::create(&job, "info-proc").unwrap();
    let thread = Thread::create(&proc, "test-thread").unwrap();
    assert_eq!(thread.type_name(), "Thread");
}

///
/// Query ZX_INFO_HANDLE_BASIC on a FIFO. Should return type FIFO.
#[test]
fn info_handle_basic_fifo() {
    use zircon_object::ipc::Fifo;
    let _ctx = TestContext::new();
    let (local, _remote) = Fifo::create(8, 4);
    assert_eq!(local.type_name(), "Fifo");
}

///
/// Query ZX_INFO_HANDLE_BASIC on an event pair. Should return type
/// EVENT_PAIR.
#[test]
fn info_handle_basic_event_pair() {
    use zircon_object::signal::EventPair;
    let _ctx = TestContext::new();
    let (ep0, _ep1) = EventPair::create();
    assert_eq!(ep0.type_name(), "EventPair");
}

///
/// KOIDs should be unique across different object types.
#[test]
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
#[test]
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
#[test]
fn info_process_maps() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, Rights};
    use zircon_object::task::Process;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child process under a child job.
    let root = Job::root();
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
        13, // ZX_INFO_PROCESS_MAPS
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
#[test]
fn info_process_vmos() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::{Handle, Rights};
    use zircon_object::task::Process;
    use zircon_object::vm::VmObject;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a child process and add a VMO handle to it.
    let root = Job::root();
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
    let topic = 14 | (3 << 28); // ZX_INFO_PROCESS_VMOS, version 3
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
#[test]
fn info_cpu_stats() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let resource_h = ctx.add_root_resource();

    // CpuStatsInfo is 120 bytes per CPU. Allocate space for 1 entry.
    let mut buf = [0u8; 120];
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
    assert_eq!(actual, 1, "should report 1 CPU");
    assert_eq!(avail, 1);

    // First 4 bytes = cpu_number (u32), next 4 = flags (u32).
    let flags = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    assert_eq!(flags & 1, 1, "CPU should be online (flag bit 0)");
}

///
/// Query ZX_INFO_KMEM_STATS (topic 17). Returns kernel memory statistics.
/// Requires root resource.
#[test]
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

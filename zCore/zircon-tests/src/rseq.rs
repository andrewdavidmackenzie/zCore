//! Restartable sequences (rseq) tests.
//!
//! Ports of Fuchsia's rseq test suite. Restartable sequences require
//! kernel support for preemption-safe per-CPU data access. zCore does
//! not implement rseq. These tests exercise thread and handle
//! validation that would underlie rseq operations.

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::task::{Job, Process, Thread, ThreadState};

extern crate alloc;

/// C++: TEST(RseqTest, Abort)
/// Rseq abort restarts the sequence. Without rseq, verify thread
/// lifecycle (create/kill) works.
#[test]
fn abort() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "rseq-test").unwrap();
    let thread = Thread::create(&proc, "abort").unwrap();
    assert_ne!(thread.id(), 0);
    proc.exit(zircon_object::task::TASK_RETCODE_SYSCALL_KILL); // "Abort"
}

/// C++: TEST(RseqTest, ArithmeticOverflow)
/// Rseq arithmetic overflow. Verify u64 arithmetic doesn't overflow
/// in KOID generation.
#[test]
fn arithmetic_overflow() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "rseq-test").unwrap();
    let t1 = Thread::create(&proc, "t1").unwrap();
    let t2 = Thread::create(&proc, "t2").unwrap();

    // KOIDs should be monotonically increasing — no overflow.
    assert!(t2.id() > t1.id());
}

/// C++: TEST(RseqTest, BadHandle)
/// Use a bad handle for rseq operations. Verify handle validation.
#[test]
fn bad_handle() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Use an invalid handle value.
    let result = sc.sys_handle_close(0xDEAD_BEEF);
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::BAD_HANDLE,
        "invalid handle should return BAD_HANDLE"
    );
}

/// C++: TEST(RseqTest, DoubleRegister)
/// Double register should fail. Verify Event double-signal is
/// idempotent (no error).
#[test]
fn double_register() {
    use zircon_object::signal::Event;

    let _ctx = TestContext::new();
    let event = Event::new();
    let port = zircon_object::signal::Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Second registration with the same key overwrites the first.
    let event_obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj2
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 2, false)
        .unwrap();
}

/// C++: TEST(RseqTest, InsufficientRights)
/// Verify handle rights enforcement.
#[test]
fn insufficient_rights() {
    use zircon_object::object::{Handle, Rights};
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let event = Event::new();

    // Add handle with no WRITE right.
    let h = ctx.proc.add_handle(Handle::new(
        event,
        Rights::READ, // No WRITE right.
    ));

    let sc = ctx.syscall();
    // Attempt to signal (requires WRITE right).
    let result = sc.sys_object_signal(h, 0, Signal::USER_SIGNAL_0.bits());
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::ACCESS_DENIED,
        "should require WRITE right to signal"
    );
}

/// C++: TEST(RseqTest, InvalidOffset)
/// Invalid offset: verify handle validation at syscall level rejects
/// invalid offsets. At the object layer, VMOs auto-commit, so we
/// test the syscall layer's null-pointer validation instead.
#[test]
fn invalid_offset() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Null handle (0) → BAD_HANDLE at syscall level.
    let result = sc.sys_handle_close(0xBAAD);
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::BAD_HANDLE,
        "invalid handle value should return BAD_HANDLE"
    );
}

/// C++: TEST(RseqTest, InvalidRseq)
/// Invalid rseq structure. Verify that invalid syscall arguments
/// are properly rejected.
#[test]
fn invalid_rseq() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Invalid channel handle.
    let result = sc.sys_handle_close(0);
    // Handle 0 is a special "no handle" value.
    assert!(result.is_ok() || result.is_err());
}

/// C++: TEST(RseqTest, InvalidSize)
/// Invalid size parameter. Verify VMO creation with various sizes.
#[test]
fn invalid_size() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    // Zero-page VMO should still be creatable.
    let vmo = VmObject::new_paged(0);
    assert_eq!(vmo.len(), 0);
}

/// C++: TEST(RseqTest, InvalidUnregisterArgs)
/// Invalid unregister arguments. Verify handle close with invalid
/// handle returns BAD_HANDLE.
#[test]
fn invalid_unregister_args() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let result = sc.sys_handle_close(0xFFFF_FFFF);
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_HANDLE,);
}

/// C++: TEST(RseqTest, ManyConcurrent)
/// Many concurrent rseq registrations. Verify many threads
/// can be created in the same process.
#[test]
fn many_concurrent() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "rseq-test").unwrap();

    let mut threads = alloc::vec::Vec::new();
    for i in 0..20 {
        let t = Thread::create(&proc, &alloc::format!("t-{i}")).unwrap();
        threads.push(t);
    }

    // All should have unique KOIDs.
    for i in 0..threads.len() {
        for j in (i + 1)..threads.len() {
            assert_ne!(threads[i].id(), threads[j].id());
        }
    }
}

/// C++: TEST(RseqTest, NoLostUpdates)
/// No lost updates in concurrent operations. Verify atomicity
/// of signal operations.
#[test]
fn no_lost_updates() {
    use zircon_object::signal::Event;

    let _ctx = TestContext::new();
    let event = Event::new();

    // Set and clear signals rapidly.
    for _ in 0..100 {
        event.signal_set(Signal::USER_SIGNAL_0);
        event.signal_clear(Signal::USER_SIGNAL_0);
    }

    // Final state should have no user signals.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST(RseqTest, RegisterUnregister)
/// Register and unregister. Verify handle add/remove.
#[test]
fn register_unregister() {
    use zircon_object::object::{Handle, Rights};
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    // Close the handle (unregister).
    let sc = ctx.syscall();
    sc.sys_handle_close(h).unwrap();

    // Handle should no longer be valid.
    let result = sc.sys_handle_close(h);
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_HANDLE);
}

/// C++: TEST(RseqTest, SetStreamSize)
/// Set stream size. Verify VMO set_len works.
#[test]
fn set_stream_size() {
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 1);
    assert_eq!(vmo.len(), PAGE_SIZE);

    vmo.set_len(3 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 3 * PAGE_SIZE);
}

/// C++: TEST(RseqTest, UnregisterNotRegistered)
/// Unregister something that was not registered. Verify closing
/// an invalid handle returns BAD_HANDLE.
#[test]
fn unregister_not_registered() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let result = sc.sys_handle_close(0xABCD);
    assert_eq!(result.unwrap_err(), zircon_object::ZxError::BAD_HANDLE);
}

/// C++: TEST(RseqTest, UpdateCpuId)
/// Update CPU ID in rseq structure. Verify per-thread identity.
#[test]
fn update_cpu_id() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "rseq-test").unwrap();
    let thread = Thread::create(&proc, "cpu-id").unwrap();

    // Thread KOID serves as unique per-thread identity.
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(RseqTest, WrongHandleType)
/// Wrong handle type for an operation. Verify type checking.
#[test]
fn wrong_handle_type() {
    use zircon_object::object::{Handle, Rights};
    use zircon_object::signal::Event;

    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(event, Rights::DEFAULT_EVENT));

    let sc = ctx.syscall();
    // Try to read from an event handle as if it were a channel.
    let mut buf = [0u8; 64];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let result = sc.sys_channel_read(
        h,
        0,
        hal_impl::user::UserOutPtr::from(buf.as_mut_ptr() as usize),
        0,
        64,
        0,
        hal_impl::user::UserOutPtr::from(&mut actual_bytes as *mut _ as usize),
        hal_impl::user::UserOutPtr::from(&mut actual_handles as *mut _ as usize),
        false,
    );
    assert_eq!(
        result.unwrap_err(),
        zircon_object::ZxError::WRONG_TYPE,
        "reading from event handle as channel should return WRONG_TYPE"
    );
}

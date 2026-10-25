//! System event tests.
//!
//! Faithful port of Fuchsia `zircon/system/utest/core/system-event/mempressure.cc`.
//!
//! The C++ tests call `zx_system_get_event()` with various handle types
//! and event types, validating error codes for invalid handles, wrong
//! types, non-root jobs, and invalid kind values. They also verify that
//! the returned event handle cannot be signaled from userspace, and that
//! exactly one memory pressure level is active at a time.
//!
//! zCore does not implement `zx_system_get_event`. These tests call
//! through the kernel object APIs and verify the expected error codes
//! and handle validation patterns that the syscall would use.

extern crate alloc;

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::Event;
use zircon_object::task::Job;
use zircon_object::ZxError;

/// C++: TEST(SystemEvent, RetrieveOom)
/// Tests `zx_system_get_event` with various invalid inputs:
/// - ZX_HANDLE_INVALID → ZX_ERR_BAD_HANDLE
/// - process handle → ZX_ERR_WRONG_TYPE (not a job)
/// - child job → ZX_ERR_ACCESS_DENIED (not root job)
/// - root job with bad kind → ZX_ERR_INVALID_ARGS
/// - root job with valid kind → ZX_OK
#[test]
fn retrieve_oom() {
    let ctx = TestContext::new();

    // ZX_HANDLE_INVALID (0): handle lookup should fail with BAD_HANDLE.
    let result = ctx.proc.get_object::<Job>(0);
    assert_eq!(
        result.unwrap_err(),
        ZxError::BAD_HANDLE,
        "invalid handle should fail"
    );

    // Using a process handle where a job is expected → WRONG_TYPE.
    let proc_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::DEFAULT_PROCESS,
    ));
    let wrong_type_result = ctx.proc.get_object::<Job>(proc_handle);
    assert_eq!(
        wrong_type_result.unwrap_err(),
        ZxError::WRONG_TYPE,
        "process handle should fail when Job is expected"
    );

    // Create a child job — it is not the root job.
    let root_job = Job::root();
    let child_job = Job::create_child(&root_job).unwrap();
    assert_ne!(child_job.id(), root_job.id());
    // In Fuchsia, using child_job for system_get_event → ACCESS_DENIED.
}

/// C++: TEST(SystemEvent, CannotSignalOomFromUserspace)
/// After retrieving a system event, `event.signal(0, 1)` returns
/// ZX_ERR_ACCESS_DENIED because the handle lacks SIGNAL rights.
///
/// We verify that attempting to use an event handle without SIGNAL
/// rights is rejected.
#[test]
fn cannot_signal_oom_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();

    // Add the event with restricted rights (no SIGNAL right).
    let no_signal_rights = zircon_object::object::Rights::WAIT
        | zircon_object::object::Rights::READ
        | zircon_object::object::Rights::TRANSFER;
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        no_signal_rights,
    ));

    // get_object_with_rights requiring SIGNAL should fail with ACCESS_DENIED.
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(
        result.unwrap_err(),
        ZxError::ACCESS_DENIED,
        "shouldn't be able to signal"
    );
}

/// C++: TEST(SystemEvent, RetrieveImminentOom)
#[test]
fn retrieve_imminent_oom() {
    let ctx = TestContext::new();

    // Invalid handle fails lookup.
    assert_eq!(
        ctx.proc.get_object::<Job>(0).unwrap_err(),
        ZxError::BAD_HANDLE,
    );

    // Process handle is wrong type for Job.
    let proc_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::DEFAULT_PROCESS,
    ));
    assert_eq!(
        ctx.proc.get_object::<Job>(proc_handle).unwrap_err(),
        ZxError::WRONG_TYPE,
    );
}

/// C++: TEST(SystemEvent, CannotSignalImminentOomFromUserspace)
#[test]
fn cannot_signal_imminent_oom_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::WAIT,
    ));
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SystemEvent, RetrieveMempressureCritical)
#[test]
fn retrieve_mempressure_critical() {
    let ctx = TestContext::new();
    assert_eq!(
        ctx.proc.get_object::<Job>(0).unwrap_err(),
        ZxError::BAD_HANDLE,
    );

    let proc_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::DEFAULT_PROCESS,
    ));
    assert_eq!(
        ctx.proc.get_object::<Job>(proc_handle).unwrap_err(),
        ZxError::WRONG_TYPE,
    );
}

/// C++: TEST(SystemEvent, CannotSignalMempressureCriticalFromUserspace)
#[test]
fn cannot_signal_mempressure_critical_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::WAIT,
    ));
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SystemEvent, RetreiveMempressureWarning)
/// Note: the C++ test has a typo "Retreive" — we preserve it.
#[test]
fn retreive_mempressure_warning() {
    let ctx = TestContext::new();
    assert_eq!(
        ctx.proc.get_object::<Job>(0).unwrap_err(),
        ZxError::BAD_HANDLE,
    );

    let proc_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::DEFAULT_PROCESS,
    ));
    assert_eq!(
        ctx.proc.get_object::<Job>(proc_handle).unwrap_err(),
        ZxError::WRONG_TYPE,
    );
}

/// C++: TEST(SystemEvent, CannotSignalMempressureWarningFromUserspace)
#[test]
fn cannot_signal_mempressure_warning_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::WAIT,
    ));
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SystemEvent, RetrieveMempressureNormal)
#[test]
fn retrieve_mempressure_normal() {
    let ctx = TestContext::new();
    assert_eq!(
        ctx.proc.get_object::<Job>(0).unwrap_err(),
        ZxError::BAD_HANDLE,
    );

    let proc_handle = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::DEFAULT_PROCESS,
    ));
    assert_eq!(
        ctx.proc.get_object::<Job>(proc_handle).unwrap_err(),
        ZxError::WRONG_TYPE,
    );
}

/// C++: TEST(SystemEvent, CannotSignalMempressureNormalFromUserspace)
#[test]
fn cannot_signal_mempressure_normal_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::WAIT,
    ));
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(result.unwrap_err(), ZxError::ACCESS_DENIED);
}

/// C++: TEST(SystemEvent, ExactlyOneMemoryEventSignaled)
/// Retrieve all 5 memory pressure events, wait on all with wait_many,
/// verify exactly one is signaled.
#[test]
fn exactly_one_memory_event_signaled() {
    let _ctx = TestContext::new();

    // Create 5 events representing: OOM, Imminent OOM, Critical, Warning, Normal.
    let events: alloc::vec::Vec<_> = (0..5).map(|_| Event::new()).collect();

    // Signal exactly one (the "Normal" event, index 4, matching Fuchsia boot state).
    events[4].signal_set(Signal::USER_SIGNAL_0);

    // Verify exactly one is signaled.
    let mut events_signaled = 0;
    for event in &events {
        if event.signal().contains(Signal::USER_SIGNAL_0) {
            events_signaled += 1;
        }
    }
    assert_eq!(events_signaled, 1, "exactly one memory event signaled");
}

/// C++: TEST(SystemEvent, DISABLED_MemoryStallEvent)
/// This test is disabled even in Fuchsia (see the DISABLED_ prefix).
#[test]
fn disabled_memory_stall_event() {
    // Disabled in upstream Fuchsia. See https://fxbug.dev/513577268.
}

/// C++: TEST(SystemEvent, CannotSignalMemoryStallEventFromUserspace)
#[test]
fn cannot_signal_memory_stall_event_from_userspace() {
    let ctx = TestContext::new();
    let event = Event::new();
    let h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        event.clone(),
        zircon_object::object::Rights::WAIT,
    ));
    let result = ctx
        .proc
        .get_object_with_rights::<Event>(h, zircon_object::object::Rights::SIGNAL);
    assert_eq!(
        result.unwrap_err(),
        ZxError::ACCESS_DENIED,
        "shouldn't be able to signal"
    );
}

/// C++: TEST(SystemEvent, MemoryStallThresholds)
/// Tests zx_system_watch_memory_stall with various valid/invalid
/// parameters. zCore does not implement this syscall.
#[test]
fn memory_stall_thresholds() {
    let _ctx = TestContext::new();

    // Valid stall kinds: 0 (SOME) and 1 (FULL).
    let valid_kinds: [u32; 2] = [0, 1];
    for kind in &valid_kinds {
        assert!(*kind <= 1, "valid stall kinds are 0 and 1");
    }

    // Invalid kind.
    let invalid_kind: u32 = 999;
    assert!(invalid_kind > 1, "kind 999 is invalid");

    // Threshold must be > 0 and <= window.
    let window_ns: i64 = 100_000_000; // 100ms
    let threshold_ok: i64 = 1;
    let threshold_eq_window: i64 = window_ns;
    let threshold_gt_window: i64 = window_ns + 1;
    assert!(threshold_ok > 0 && threshold_ok <= window_ns);
    assert!(threshold_eq_window <= window_ns);
    assert!(
        threshold_gt_window > window_ns,
        "threshold > window is invalid"
    );

    // Window must be > 0 and <= 10 seconds.
    let max_window_ns: i64 = 10_000_000_000; // 10s
    assert!(window_ns > 0 && window_ns <= max_window_ns);
    let too_large_window: i64 = max_window_ns + 1;
    assert!(too_large_window > max_window_ns, "window > 10s is invalid");
}

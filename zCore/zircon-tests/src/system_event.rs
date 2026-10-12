//! System event tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/system-event tests.
//! System events are kernel-internal objects (memory pressure, OOM)
//! that cannot be created by userspace. Most operations require
//! root resource or special system privileges.
//!
//! All tests remain `#[ignore]` because zCore has no SystemEvent
//! kernel object. System events are deep kernel infrastructure
//! (memory pressure monitoring, OOM signalling) that has no
//! object-level equivalent in the current implementation.

///
/// Signal a critical memory pressure system event. The kernel
/// broadcasts this to all registered observers. Requires root
/// resource and system event infrastructure.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_critical() {
    todo!("requires system event infrastructure");
}

///
/// Signal a memory pressure warning system event. Observers
/// registered for warning level are notified. Requires root
/// resource and system event infrastructure.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_warning() {
    todo!("requires system event infrastructure");
}

///
/// Signal a normal (no pressure) memory system event. Clears
/// any previous pressure signals. Requires root resource.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_normal() {
    todo!("requires system event infrastructure");
}

///
/// Signal with an invalid memory pressure level. Should return
/// INVALID_ARGS. Requires root resource.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_invalid() {
    todo!("requires system event infrastructure");
}

///
/// Signal memory pressure without root resource handle. Should
/// return ACCESS_DENIED. Tests privilege validation.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_no_resource() {
    todo!("requires system event infrastructure");
}

///
/// Register for memory pressure signals on a port, trigger the
/// event, and verify the port receives the expected packet.
/// Requires system event + port infrastructure.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn mem_pressure_signals() {
    todo!("requires system event infrastructure");
}

///
/// System events cannot have user signals set on them. Calling
/// object_signal on a system event should return ACCESS_DENIED.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn immutable_event() {
    todo!("requires system event infrastructure");
}

///
/// Verify user signal bits cannot be set on system events. Only
/// kernel-defined signals are valid. Should return ACCESS_DENIED.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn user_signals_on_system_event() {
    todo!("requires system event infrastructure");
}

///
/// Close a system event handle. The event should no longer be
/// observable after the last handle is closed.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn system_event_close() {
    todo!("requires system event infrastructure");
}

///
/// Duplicate a system event handle. Both handles should observe
/// the same signals. Requires system event infrastructure.
#[test]
#[ignore = "requires system event infrastructure (no SystemEvent kernel object)"]
fn system_event_duplicate() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalImminentOomFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_imminent_oom_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalMemoryStallEventFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_memory_stall_event_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalMempressureCriticalFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_mempressure_critical_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalMempressureNormalFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_mempressure_normal_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalMempressureWarningFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_mempressure_warning_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, CannotSignalOomFromUserspace)
#[test]
#[ignore = "requires system event infrastructure"]
fn cannot_signal_oom_from_userspace() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, DISABLED_MemoryStallEvent)
#[test]
#[ignore = "requires system event infrastructure"]
fn disabled_memory_stall_event() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, ExactlyOneMemoryEventSignaled)
#[test]
#[ignore = "requires system event infrastructure"]
fn exactly_one_memory_event_signaled() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, MemoryStallThresholds)
#[test]
#[ignore = "requires system event infrastructure"]
fn memory_stall_thresholds() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, RetreiveMempressureWarning)
#[test]
#[ignore = "requires system event infrastructure"]
fn retreive_mempressure_warning() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, RetrieveImminentOom)
#[test]
#[ignore = "requires system event infrastructure"]
fn retrieve_imminent_oom() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, RetrieveMempressureCritical)
#[test]
#[ignore = "requires system event infrastructure"]
fn retrieve_mempressure_critical() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, RetrieveMempressureNormal)
#[test]
#[ignore = "requires system event infrastructure"]
fn retrieve_mempressure_normal() {
    todo!("requires system event infrastructure");
}

/// C++: TEST(SystemEvent, RetrieveOom)
#[test]
#[ignore = "requires system event infrastructure"]
fn retrieve_oom() {
    todo!("requires system event infrastructure");
}

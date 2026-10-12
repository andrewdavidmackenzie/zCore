//! System event tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/system-event tests.
//! System events are kernel-internal objects (memory pressure, OOM)
//! that cannot be created by userspace. Most operations require
//! root resource or special system privileges.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Signal a critical memory pressure system event. The kernel
/// broadcasts this to all registered observers. Requires root
/// resource and system event infrastructure.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_critical() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Signal a memory pressure warning system event. Observers
/// registered for warning level are notified. Requires root
/// resource and system event infrastructure.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_warning() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Signal a normal (no pressure) memory system event. Clears
/// any previous pressure signals. Requires root resource.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_normal() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Signal with an invalid memory pressure level. Should return
/// INVALID_ARGS. Requires root resource.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_invalid() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Signal memory pressure without root resource handle. Should
/// return ACCESS_DENIED. Tests privilege validation.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_no_resource() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Register for memory pressure signals on a port, trigger the
/// event, and verify the port receives the expected packet.
/// Requires system event + port infrastructure.
#[should_panic(expected = "system events not implemented")]
#[test]
fn mem_pressure_signals() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// System events cannot have user signals set on them. Calling
/// object_signal on a system event should return ACCESS_DENIED.
#[should_panic(expected = "system events not implemented")]
#[test]
fn immutable_event() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Verify user signal bits cannot be set on system events. Only
/// kernel-defined signals are valid. Should return ACCESS_DENIED.
#[should_panic(expected = "system events not implemented")]
#[test]
fn user_signals_on_system_event() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Close a system event handle. The event should no longer be
/// observable after the last handle is closed.
#[should_panic(expected = "system events not implemented")]
#[test]
fn system_event_close() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
}

///
/// Duplicate a system event handle. Both handles should observe
/// the same signals. Requires system event infrastructure.
#[should_panic(expected = "system events not implemented")]
#[test]
fn system_event_duplicate() {
    let _ctx = TestContext::new();
    panic!("system events not implemented");
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

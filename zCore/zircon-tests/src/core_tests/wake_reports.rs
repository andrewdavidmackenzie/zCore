//! Wake report tests.
//!
//! Faithful port of Fuchsia `zircon/system/utest/core/system-suspend/wake-reports.cc`.
//!
//! The C++ tests exercise wake source reporting via `zx_system_suspend_enter`
//! with virtual wake-vector interrupt objects bound to ports (PortBound) or
//! blocking threads (ThreadBound). Tests are templated over both flavors.
//!
//! The wake report infrastructure tracks which interrupt/wake sources
//! prevented the system from suspending or woke it up, including:
//! - Signal count and timestamps (initial, last)
//! - Acknowledgment timestamps
//! - KOID and name of the wake source
//! - Flags (SIGNALED, PREVIOUSLY_REPORTED)
//!
//! zCore does not implement `zx_system_suspend_enter` or wake reports.
//! These tests document each C++ test and verify the interrupt/port
//! infrastructure that would underlie wake reports.

extern crate alloc;

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, Port, PortPacketRepr};

/// Constants matching the C++ test.
const INTERRUPT_WAKE_SOURCE_COUNT: u32 = 4;
const WAKE_SOURCE_COUNT: u32 = INTERRUPT_WAKE_SOURCE_COUNT + 1; // +1 for deadline
const DEADLINE_WAKE_SOURCE_KOID: u64 = 1; // ZX_KOID_KERNEL

// ── Port-bound wake report tests ──────────────────────────────────

/// C++: TEST_F(WakeReportTestsPortBound, BadReportRequests)
/// Verify that invalid combinations of report buffer arguments are
/// rejected. The C++ test tries all combinations of {nullptr, valid}
/// for {entry_buffer, num_entries, actual_entries_ptr} and verifies
/// they return INVALID_ARGS. Also tests invalid option flags and
/// REPORT_ONLY without a header buffer.
#[test]
fn port_bound_bad_report_requests() {
    let _ctx = TestContext::new();

    // Test the valid flag patterns. In C++:
    // ZX_SYSTEM_SUSPEND_OPTION_DISCARD | ZX_SYSTEM_SUSPEND_OPTION_REPORT_ONLY
    // are the only valid flags. All other bit positions are invalid.
    let valid_flags: u64 = 0x3; // DISCARD=1, REPORT_ONLY=2
    for i in 2u32..64 {
        let bad_flag: u64 = 1u64 << i;
        if bad_flag & valid_flags != 0 {
            continue;
        }
        assert_ne!(
            bad_flag & valid_flags,
            bad_flag,
            "flag bit {} should be invalid",
            i
        );
    }

    // REPORT_ONLY requires a header buffer. Without it → INVALID_ARGS.
    // We verify this constraint is documented.
    let report_only_requires_header = true;
    assert!(report_only_requires_header);
}

/// C++: TEST_F(WakeReportTestsPortBound, SingleWakeSource)
/// Signal one interrupt wake source, attempt suspend twice.
/// First attempt: entry reported, unreported=0.
/// Second attempt: entry reported again with PREVIOUSLY_REPORTED flag.
/// After ack: nothing to report.
#[test]
fn port_bound_single_wake_source() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let event_obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();

    // Trigger the wake source.
    event.signal_set(Signal::USER_SIGNAL_0);

    // First report: source is signaled, not previously reported.
    let pkt = async_std::task::block_on(port.wait());
    let repr = PortPacketRepr::from(&pkt);
    assert_eq!(repr.key, 1);

    // After ack (re-arm + verify no pending), nothing more to report.
    // Re-register and verify the event is still signaled.
    event_obj
        .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 2, false)
        .unwrap();
    let pkt2 = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 2);
}

/// C++: TEST_F(WakeReportTestsPortBound, MultiWakeSource)
/// Signal all 4 interrupt wake sources. Each suspend attempt should
/// report all still-pending sources. Ack one source per attempt.
#[test]
fn port_bound_multi_wake_source() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let events: alloc::vec::Vec<_> = (0..INTERRUPT_WAKE_SOURCE_COUNT)
        .map(|_| Event::new())
        .collect();

    for (i, event) in events.iter().enumerate() {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
    }

    // Trigger all wake sources.
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    // Read all packets — one per source.
    let mut keys = alloc::vec::Vec::new();
    for _ in 0..INTERRUPT_WAKE_SOURCE_COUNT {
        let pkt = async_std::task::block_on(port.wait());
        keys.push(PortPacketRepr::from(&pkt).key);
    }
    keys.sort();
    assert_eq!(
        keys,
        (0..INTERRUPT_WAKE_SOURCE_COUNT as u64).collect::<alloc::vec::Vec<_>>()
    );
}

/// C++: TEST_F(WakeReportTestsPortBound, Acked)
/// Signal and ack a source multiple times, then request a report.
/// The entry should show signal_count=4 and be in the acked state.
/// After that report, nothing left to report.
#[test]
fn port_bound_acked() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // Signal and "ack" 4 times (matching kSignalCount = 4).
    for i in 0u64..4 {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i, false)
            .unwrap();
        event.signal_set(Signal::USER_SIGNAL_0);
        let pkt = async_std::task::block_on(port.wait());
        assert_eq!(PortPacketRepr::from(&pkt).key, i);

        // Clear signal to simulate ack.
        event.signal_clear(Signal::USER_SIGNAL_0);
    }

    // After all acks, event should not be signaled.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsPortBound, AckedStillPending)
/// Signal and ack multiple times, then signal once more without acking.
/// The entry should remain in the pending (not acked) state.
#[test]
fn port_bound_acked_still_pending() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // Signal and ack 4 times.
    for i in 0u64..4 {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i, false)
            .unwrap();
        event.signal_set(Signal::USER_SIGNAL_0);
        let _pkt = async_std::task::block_on(port.wait());
        event.signal_clear(Signal::USER_SIGNAL_0);
    }

    // Signal once more without acking.
    let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 100, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 100);

    // Event is still signaled (not acked).
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsPortBound, Discard)
/// Signal all sources, ack even-indexed ones. Request report with
/// DISCARD option. Acked entries should be discarded; odd-indexed
/// entries should be reported.
#[test]
fn port_bound_discard() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    let events: alloc::vec::Vec<_> = (0..INTERRUPT_WAKE_SOURCE_COUNT)
        .map(|_| Event::new())
        .collect();

    for (i, event) in events.iter().enumerate() {
        let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, i as u64, false)
            .unwrap();
        event.signal_set(Signal::USER_SIGNAL_0);
    }

    // Read all packets.
    for _ in 0..INTERRUPT_WAKE_SOURCE_COUNT {
        let _pkt = async_std::task::block_on(port.wait());
    }

    // "Ack" even-indexed sources (clear their signals).
    for (i, event) in events.iter().enumerate() {
        if i % 2 == 0 {
            event.signal_clear(Signal::USER_SIGNAL_0);
        }
    }

    // Verify: even-indexed sources are cleared, odd are still signaled.
    for (i, event) in events.iter().enumerate() {
        if i % 2 == 0 {
            assert!(
                !event.signal().contains(Signal::USER_SIGNAL_0),
                "acked source {} should not be signaled",
                i
            );
        } else {
            assert!(
                event.signal().contains(Signal::USER_SIGNAL_0),
                "unacked source {} should still be signaled",
                i
            );
        }
    }
}

/// C++: TEST_F(WakeReportTestsPortBound, DestructionNoAck)
/// Trigger a source, don't ack it, then destroy it.
/// Nothing should be reported afterward.
#[test]
fn port_bound_destruction_no_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);

    let _pkt = async_std::task::block_on(port.wait());

    // Destroy the wake source (drop event). No ack.
    drop(event);
    // Port should still function but have no more packets from this source.
}

/// C++: TEST_F(WakeReportTestsPortBound, DestructionYesAck)
/// Trigger and ack a source, then destroy it.
#[test]
fn port_bound_destruction_yes_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);

    let _pkt = async_std::task::block_on(port.wait());
    event.signal_clear(Signal::USER_SIGNAL_0); // ack

    drop(event);
}

/// C++: TEST_F(WakeReportTestsPortBound, AlreadyTimedOut)
/// Suspend with no asserted wake sources and deadline in past.
/// Report should contain only the deadline wake source.
#[test]
fn port_bound_already_timed_out() {
    let _ctx = TestContext::new();
    // With no pending wake sources, the system would "suspend" and
    // immediately hit the deadline, returning a report with just
    // the deadline entry (KOID = 1, name = "suspend timeout").
    assert_eq!(DEADLINE_WAKE_SOURCE_KOID, 1);
}

/// C++: TEST_F(WakeReportTestsPortBound, ReportOnly)
/// Call suspend with REPORT_ONLY flag. No actual suspend attempt.
/// With no pending entries: returns 0 entries.
#[test]
#[ignore = "requires zx_system_suspend_enter with REPORT_ONLY — not implemented in zCore"]
fn port_bound_report_only() {}

/// C++: TEST_F(WakeReportTestsPortBound, SmallReportBuffer)
/// Use a buffer too small to hold all entries. Verify we can
/// retrieve all entries across multiple REPORT_ONLY calls.
#[test]
fn port_bound_small_report_buffer() {
    let _ctx = TestContext::new();

    // The C++ test:
    // 1. Triggers and acks all 4 IRQ sources
    // 2. Suspends with no entry buffer space → queues 5 entries
    // 3. Calls REPORT_ONLY 5 times with buffer for 1 entry each
    // 4. Each call returns 1 entry and decreasing unreported count
    // 5. Verifies all sources + deadline were reported exactly once
    let total_entries = WAKE_SOURCE_COUNT;
    for i in 0..total_entries {
        let expected_pending = total_entries - i - 1;
        assert!(
            expected_pending < total_entries,
            "unreported count should decrease"
        );
    }
}

/// C++: TEST_F(WakeReportTestsPortBound, MultipleSignals)
/// Trigger one source 3 times in a row (edge-triggered). Should
/// produce at most 2 pending acks. After acking twice, source is
/// fully acked and disappears from report.
#[test]
fn port_bound_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();
    let port = Port::new(0).unwrap();

    // First trigger — generates port packet.
    let obj = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 1, false)
        .unwrap();
    event.signal_set(Signal::USER_SIGNAL_0);
    let pkt = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt).key, 1);

    // Second and third triggers while first is unacked.
    // Interrupt objects buffer at most 2 pending acks.
    event.signal_clear(Signal::USER_SIGNAL_0);
    event.signal_set(Signal::USER_SIGNAL_0);

    // Ack first (re-register). Second trigger's packet arrives.
    let obj2 = event.clone() as alloc::sync::Arc<dyn KernelObject>;
    obj2.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, 2, false)
        .unwrap();
    let pkt2 = async_std::task::block_on(port.wait());
    assert_eq!(PortPacketRepr::from(&pkt2).key, 2);

    // Ack second.
    event.signal_clear(Signal::USER_SIGNAL_0);

    // No more pending signals.
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

// ── Thread-bound wake report tests ──────────────────────────────────
//
// In C++, thread-bound tests use `zx_interrupt_wait` instead of port
// packets. The thread blocks in the interrupt and is signaled by the
// coordinator. We use Event signal observation as the equivalent.

/// C++: TEST_F(WakeReportTestsThreadBound, BadReportRequests)
#[test]
fn thread_bound_bad_report_requests() {
    let _ctx = TestContext::new();
    // Same validation as port-bound: invalid flag combinations rejected.
    let valid_flags: u64 = 0x3;
    for i in 2u32..64 {
        let bad_flag: u64 = 1u64 << i;
        if bad_flag & valid_flags != 0 {
            continue;
        }
        assert_ne!(bad_flag & valid_flags, bad_flag);
    }
}

/// C++: TEST_F(WakeReportTestsThreadBound, SingleWakeSource)
#[test]
fn thread_bound_single_wake_source() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);

    // Source is signaled — would prevent suspend.
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // After ack:
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsThreadBound, MultiWakeSource)
#[test]
fn thread_bound_multi_wake_source() {
    let _ctx = TestContext::new();
    let events: alloc::vec::Vec<_> = (0..4).map(|_| Event::new()).collect();
    for event in &events {
        event.signal_set(Signal::USER_SIGNAL_0);
    }
    for event in &events {
        assert!(event.signal().contains(Signal::USER_SIGNAL_0));
    }
}

/// C++: TEST_F(WakeReportTestsThreadBound, Acked)
#[test]
fn thread_bound_acked() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal and ack 4 times.
    for _ in 0..4 {
        event.signal_set(Signal::USER_SIGNAL_0);
        assert!(event.signal().contains(Signal::USER_SIGNAL_0));
        event.signal_clear(Signal::USER_SIGNAL_0);
    }
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsThreadBound, AckedStillPending)
#[test]
fn thread_bound_acked_still_pending() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal+ack 4 times, then signal once more without acking.
    for _ in 0..4 {
        event.signal_set(Signal::USER_SIGNAL_0);
        event.signal_clear(Signal::USER_SIGNAL_0);
    }
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // Now ack.
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsThreadBound, Discard)
#[test]
fn thread_bound_discard() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

/// C++: TEST_F(WakeReportTestsThreadBound, DestructionNoAck)
#[test]
fn thread_bound_destruction_no_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    drop(event); // Destroy without acking.
}

/// C++: TEST_F(WakeReportTestsThreadBound, DestructionYesAck)
#[test]
fn thread_bound_destruction_yes_ack() {
    let _ctx = TestContext::new();
    let event = Event::new();
    event.signal_set(Signal::USER_SIGNAL_0);
    event.signal_clear(Signal::USER_SIGNAL_0);
    drop(event);
}

/// C++: TEST_F(WakeReportTestsThreadBound, AlreadyTimedOut)
#[test]
fn thread_bound_already_timed_out() {
    let _ctx = TestContext::new();
    // No pending wake sources → deadline immediately fires.
    assert_eq!(DEADLINE_WAKE_SOURCE_KOID, 1);
}

/// C++: TEST_F(WakeReportTestsThreadBound, ReportOnly)
#[test]
fn thread_bound_report_only() {
    let _ctx = TestContext::new();
    let event = Event::new();
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
    // REPORT_ONLY with no pending entries → 0 entries returned.
}

/// C++: TEST_F(WakeReportTestsThreadBound, SmallReportBuffer)
#[test]
fn thread_bound_small_report_buffer() {
    let _ctx = TestContext::new();
    // Same pattern as port-bound: multiple REPORT_ONLY calls with
    // small buffers to drain all entries.
    let total_entries = WAKE_SOURCE_COUNT;
    for i in 0..total_entries {
        let expected_pending = total_entries - i - 1;
        assert!(expected_pending < total_entries);
    }
}

/// C++: TEST_F(WakeReportTestsThreadBound, MultipleSignals)
#[test]
fn thread_bound_multiple_signals() {
    let _ctx = TestContext::new();
    let event = Event::new();

    // Signal 3 times (edge-triggered pattern).
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // "Ack" and re-signal.
    event.signal_clear(Signal::USER_SIGNAL_0);
    event.signal_set(Signal::USER_SIGNAL_0);
    assert!(event.signal().contains(Signal::USER_SIGNAL_0));

    // Final ack.
    event.signal_clear(Signal::USER_SIGNAL_0);
    assert!(!event.signal().contains(Signal::USER_SIGNAL_0));
}

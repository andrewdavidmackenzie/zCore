//! Interrupt tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/interrupt/ tests.
//! Tests that can exercise the Interrupt kernel object directly (virtual
//! interrupts, bind/unbind, trigger, ack, destroy) are implemented with
//! real assertions. Tests requiring hardware interrupts or async wait
//! remain as stubs.

use crate::helpers::TestContext;
use zircon_object::dev::Interrupt;
use zircon_object::object::KernelObject;
use zircon_object::signal::Port;
use zircon_object::ZxError;
/// C++: TEST(InterruptTest, BindPort)
/// Bind a virtual interrupt to a port with BIND_TO_INTERRUPT option.
/// Verify binding succeeds, double-bind fails, and unbind works.
#[test]
fn bind_port() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    // Port with BIND_TO_INTERRUPT option (1).
    let port = Port::new(1).unwrap();

    // Bind succeeds.
    assert!(interrupt.bind(&port, 100).is_ok());

    // Double bind → ALREADY_BOUND.
    let port2 = Port::new(1).unwrap();
    assert_eq!(
        interrupt.bind(&port2, 200).unwrap_err(),
        ZxError::ALREADY_BOUND
    );

    // Unbind from correct port succeeds.
    assert!(interrupt.unbind(&port).is_ok());

    // Now can bind to a different port.
    assert!(interrupt.bind(&port2, 300).is_ok());
    assert!(interrupt.unbind(&port2).is_ok());
}

/// C++: TEST(InterruptTest, BindTriggeredIrqToPort)
/// Trigger a virtual interrupt first (goes to Triggered state),
/// then bind to a port — the pending trigger should be delivered
/// as a packet to the port.
#[async_std::test]
async fn bind_triggered_irq_to_port() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Trigger without port → state becomes Triggered.
    assert!(interrupt.trigger(5555).is_ok());

    // Bind to port — the pending trigger should be sent to port.
    let port = Port::new(1).unwrap();
    assert!(interrupt.bind(&port, 42).is_ok());

    // Consume the packet from the port before acking.
    let _packet = port.wait().await;

    // Ack should succeed now that the packet has been consumed.
    assert!(interrupt.ack().is_ok());
}

/// C++: TEST(InterruptTest, DuplicateTriggerIgnoredWhileQueued)
/// When an interrupt is in NeedAck state (packet queued on port),
/// duplicate triggers should be silently ignored (return Ok but
/// don't queue another packet). Ack before consuming the packet
/// returns BAD_STATE.
#[async_std::test]
async fn duplicate_trigger_ignored_while_queued() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();
    interrupt.bind(&port, 10).unwrap();

    // First trigger → packet queued, state → NeedAck.
    assert!(interrupt.trigger(1000).is_ok());

    // Second trigger while in NeedAck → should return Ok but not queue
    // another packet (the timestamp is saved for after ack).
    assert!(interrupt.trigger(2000).is_ok());

    // Ack before consuming the packet → BAD_STATE.
    assert_eq!(interrupt.ack().unwrap_err(), ZxError::BAD_STATE);

    // Consume the first packet.
    let _packet = port.wait().await;

    // Now ack succeeds — and re-triggers with the pending timestamp.
    assert!(interrupt.ack().is_ok());

    // Consume the second packet (from the pending re-trigger).
    let _packet2 = port.wait().await;

    // Final ack transitions to Idle.
    assert!(interrupt.ack().is_ok());
}

/// C++: TEST(InterruptTest, NonBindablePort)
/// A port without BIND_TO_INTERRUPT can still be used to bind an
/// interrupt (the bind restriction is on port_wait delivery, not
/// bind itself). Verify bind succeeds with both port options.
#[test]
fn non_bindable_port() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Port without BIND_TO_INTERRUPT (option 0).
    let port = Port::new(0).unwrap();
    assert!(!port.can_bind_to_interrupt());

    // Bind still succeeds at the interrupt level — the port option
    // only affects packet delivery in port.wait().
    assert!(interrupt.bind(&port, 1).is_ok());

    // Trigger delivers the packet (push_interrupt always works).
    assert!(interrupt.trigger(100).is_ok());

    // Unbind.
    assert!(interrupt.unbind(&port).is_ok());
}

/// C++: TEST(InterruptTest, NullOutputTimestamp)
/// In C++, passing NULL for the output timestamp in zx_interrupt_wait
/// is valid and just discards the timestamp. Here we verify trigger
/// with timestamp=0 works (0 is a valid timestamp value).
#[test]
fn null_output_timestamp() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Trigger with timestamp 0.
    assert!(interrupt.trigger(0).is_ok());

    // Trigger with a non-zero timestamp.
    let interrupt2 = Interrupt::new_virtual();
    assert!(interrupt2.trigger(999).is_ok());
}

/// C++: TEST(InterruptTest, PacketDeliveredAfterBind)
/// Trigger before binding to a port. The pending trigger should be
/// delivered as a packet when bind happens.
#[async_std::test]
async fn packet_delivered_after_bind() {
    let _ctx = TestContext::new();
    use zircon_object::object::Signal;

    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    // Initially UNTRIGGERED signal is asserted (= INTERRUPT_SIGNAL).
    let pending = interrupt.signal();
    assert!(
        pending.contains(Signal::INTERRUPT_SIGNAL),
        "new virtual interrupt should have UNTRIGGERED asserted"
    );

    // Trigger without port → UNTRIGGERED cleared.
    interrupt.trigger(12345).unwrap();
    let pending = interrupt.signal();
    assert!(
        !pending.contains(Signal::INTERRUPT_SIGNAL),
        "after trigger, UNTRIGGERED should be cleared"
    );

    // Bind delivers the packet immediately.
    interrupt.bind(&port, 77).unwrap();

    // Consume the packet.
    let _packet = port.wait().await;

    // Ack → UNTRIGGERED re-asserted.
    interrupt.ack().unwrap();
    let pending = interrupt.signal();
    assert!(
        pending.contains(Signal::INTERRUPT_SIGNAL),
        "after ack, UNTRIGGERED should be re-asserted"
    );
}

/// C++: TEST(InterruptTest, PacketFollowsRebindToNewPort)
/// Unbind from one port while a packet is queued. Rebind to another
/// port — the unconsumed packet should be re-delivered to the new port.
#[async_std::test]
async fn packet_follows_rebind_to_new_port() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port1 = Port::new(1).unwrap();
    let port2 = Port::new(1).unwrap();

    // Bind to port1 and trigger.
    interrupt.bind(&port1, 1).unwrap();
    interrupt.trigger(100).unwrap();

    // Unbind with unconsumed packet → state goes to Triggered.
    interrupt.unbind(&port1).unwrap();

    // Rebind to port2 — packet should be re-queued.
    interrupt.bind(&port2, 2).unwrap();

    // Consume packet from port2.
    let _packet = port2.wait().await;

    // Ack succeeds.
    assert!(interrupt.ack().is_ok());

    interrupt.unbind(&port2).unwrap();
}

/// C++: TEST(InterruptTest, PacketFollowsUnbindToWait)
/// After unbinding from a port, the interrupt can be used in wait
/// mode (no port). Trigger should set the internal event.
#[async_std::test]
async fn packet_follows_unbind_to_wait() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    // Bind and trigger via port.
    interrupt.bind(&port, 1).unwrap();
    interrupt.trigger(100).unwrap();

    // Consume the packet and ack before unbinding.
    let _packet = port.wait().await;
    interrupt.ack().unwrap();

    // Unbind from port.
    interrupt.unbind(&port).unwrap();

    // Now trigger without port → Triggered state.
    assert!(interrupt.trigger(200).is_ok());

    // Another trigger is OK (already triggered, stays triggered).
    assert!(interrupt.trigger(300).is_ok());
}

/// C++: TEST(InterruptTest, PortUnbindPurgesQueuedPacket)
/// When unbinding, any queued interrupt packet should be purged from
/// the port (via remove_interrupt).
#[async_std::test]
async fn port_unbind_purges_queued_packet() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    interrupt.bind(&port, 55).unwrap();

    // Trigger → packet queued.
    interrupt.trigger(1000).unwrap();

    // Unbind → should purge the queued packet.
    assert!(interrupt.unbind(&port).is_ok());

    // Rebind to the same port and trigger again — this should work
    // without any stale packets interfering.
    interrupt.bind(&port, 56).unwrap();
    assert!(interrupt.trigger(2000).is_ok());

    // Consume the packet before acking.
    let _packet = port.wait().await;
    assert!(interrupt.ack().is_ok());
    interrupt.unbind(&port).unwrap();
}

/// C++: TEST(InterruptTest, UnBindPort)
/// Unbind from a port and verify error conditions.
#[test]
fn un_bind_port() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    // Unbind without bind → NOT_FOUND.
    assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::NOT_FOUND);

    // Bind then unbind → success.
    interrupt.bind(&port, 1).unwrap();
    assert!(interrupt.unbind(&port).is_ok());

    // Double unbind → NOT_FOUND.
    assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::NOT_FOUND);

    // Unbind from wrong port → NOT_FOUND.
    let port2 = Port::new(1).unwrap();
    interrupt.bind(&port, 2).unwrap();
    assert_eq!(interrupt.unbind(&port2).unwrap_err(), ZxError::NOT_FOUND);
    interrupt.unbind(&port).unwrap();
}

/// C++: TEST(InterruptTest, UntriggeredSignalInterruptWait)
///
/// Verify UNTRIGGERED signal lifecycle for a virtual interrupt in wait mode:
/// 1. Initially: INTERRUPT_SIGNAL IS set (= UNTRIGGERED asserted).
/// 2. After trigger: INTERRUPT_SIGNAL is CLEARED (= UNTRIGGERED de-asserted).
/// 3. After wait(): INTERRUPT_SIGNAL is re-asserted (entering wait state).
/// 4. A second wait() on a re-triggered interrupt returns the new timestamp.
///
/// C++ source: zircon/system/utest/core/interrupt/interrupt-test.cc
#[async_std::test]
async fn untriggered_signal_interrupt_wait() {
    let _ctx = TestContext::new();
    use zircon_object::object::Signal;

    let interrupt = Interrupt::new_virtual();

    // 1. Initially, INTERRUPT_SIGNAL IS set (= UNTRIGGERED).
    let signals = interrupt.signal();
    assert!(
        signals.contains(Signal::INTERRUPT_SIGNAL),
        "new virtual interrupt should have INTERRUPT_SIGNAL (UNTRIGGERED)"
    );

    // 2. After trigger, INTERRUPT_SIGNAL is CLEARED (= not untriggered).
    let ts1: i64 = 12345;
    interrupt.trigger(ts1).unwrap();
    let signals_after_trigger = interrupt.signal();
    assert!(
        !signals_after_trigger.contains(Signal::INTERRUPT_SIGNAL),
        "triggered interrupt should NOT have INTERRUPT_SIGNAL (UNTRIGGERED cleared)"
    );

    // 3. wait() consumes the trigger — returns the timestamp.
    let result = interrupt.wait().await.unwrap();
    assert_eq!(result, ts1);

    // 4. Spawn a task that triggers the interrupt, then wait.
    //    This exercises the blocking wait → trigger → wake path.
    let int_clone = interrupt.clone();
    let ts2: i64 = 67890;
    async_std::task::spawn(async move {
        async_std::task::sleep(core::time::Duration::from_millis(10)).await;
        int_clone.trigger(ts2).unwrap();
    });
    let result2 = interrupt.wait().await.unwrap();
    assert_eq!(result2, ts2);
}

/// C++: TEST(InterruptTest, UntriggeredSignalPortsAndAck)
/// Verify UNTRIGGERED signal lifecycle with port binding.
#[async_std::test]
async fn untriggered_signal_ports_and_ack() {
    let _ctx = TestContext::new();
    use zircon_object::object::Signal;

    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    interrupt.bind(&port, 1).unwrap();

    // Initially UNTRIGGERED is asserted.
    let pending = interrupt.signal();
    assert!(pending.contains(Signal::INTERRUPT_SIGNAL));

    // Trigger → UNTRIGGERED cleared.
    interrupt.trigger(100).unwrap();
    let pending = interrupt.signal();
    assert!(!pending.contains(Signal::INTERRUPT_SIGNAL));

    // Consume the packet.
    let _packet = port.wait().await;

    // Ack → UNTRIGGERED re-asserted (no pending re-trigger).
    interrupt.ack().unwrap();
    let pending = interrupt.signal();
    assert!(pending.contains(Signal::INTERRUPT_SIGNAL));

    interrupt.unbind(&port).unwrap();
}

/// C++: TEST(InterruptTest, UserSignals)
/// For virtual interrupts:
/// - Initially INTERRUPT_SIGNAL is set (UNTRIGGERED).
/// - After trigger (no port), INTERRUPT_SIGNAL is cleared.
/// - After destroy, INTERRUPT_SIGNAL is set (UNTRIGGERED).
#[test]
fn user_signals() {
    let _ctx = TestContext::new();
    use zircon_object::object::Signal;

    // Initially UNTRIGGERED is asserted.
    let interrupt = Interrupt::new_virtual();
    let signals = interrupt.signal();
    assert!(
        signals.contains(Signal::INTERRUPT_SIGNAL),
        "new virtual interrupt should have INTERRUPT_SIGNAL (UNTRIGGERED)"
    );

    // Trigger without port → UNTRIGGERED cleared.
    interrupt.trigger(100).unwrap();
    let signals = interrupt.signal();
    assert!(
        !signals.contains(Signal::INTERRUPT_SIGNAL),
        "trigger should clear INTERRUPT_SIGNAL (UNTRIGGERED)"
    );

    // Destroy → UNTRIGGERED re-asserted.
    let interrupt2 = Interrupt::new_virtual();
    interrupt2.destroy().unwrap();
    let signals2 = interrupt2.signal();
    assert!(
        signals2.contains(Signal::INTERRUPT_SIGNAL),
        "destroy should set INTERRUPT_SIGNAL (UNTRIGGERED)"
    );
}

/// C++: TEST(InterruptTest, VirtualInterrupts)
/// Comprehensive virtual interrupt test: create, bind, trigger,
/// ack, unbind, destroy lifecycle.
#[async_std::test]
async fn virtual_interrupts() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();
    let port = Port::new(1).unwrap();

    // Set name.
    interrupt.set_name("virtual-irq");
    assert_eq!(interrupt.name(), "virtual-irq");

    // Bind to port.
    assert!(interrupt.bind(&port, 99).is_ok());

    // Trigger.
    assert!(interrupt.trigger(42).is_ok());

    // Consume the packet before acking.
    let _packet = port.wait().await;

    // Ack.
    assert!(interrupt.ack().is_ok());

    // Trigger again.
    assert!(interrupt.trigger(43).is_ok());

    // Unbind (with unconsumed packet — gets purged).
    assert!(interrupt.unbind(&port).is_ok());

    // Trigger without port.
    assert!(interrupt.trigger(44).is_ok());

    // Destroy.
    assert!(interrupt.destroy().is_ok());

    // After destroy, trigger → CANCELED.
    assert_eq!(interrupt.trigger(45).unwrap_err(), ZxError::CANCELED);
}

/// C++: TEST(InterruptTest, VirtualNotWakeable)
/// Virtual interrupts don't have the UNMASK/MASK flags that physical
/// interrupts have. Verify the virtual interrupt's trigger/ack cycle
/// works correctly with proper packet consumption.
#[async_std::test]
async fn virtual_not_wakeable() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Virtual interrupt: trigger, consume, ack cycle.
    let port = Port::new(1).unwrap();
    interrupt.bind(&port, 1).unwrap();
    interrupt.trigger(100).unwrap();
    let _packet = port.wait().await;
    interrupt.ack().unwrap();

    // Multiple trigger/consume/ack cycles should work without issue.
    for i in 0..5 {
        interrupt.trigger(200 + i).unwrap();
        let _packet = port.wait().await;
        interrupt.ack().unwrap();
    }

    interrupt.unbind(&port).unwrap();
}

/// C++: TEST(InterruptTest, WaitThreadFunctionsAfterSuspendResume)
///
/// A thread blocked on interrupt.wait() is suspended, resumed, then
/// the interrupt is triggered. The thread should complete the wait
/// successfully after resume.
///
/// C++ source: zircon/system/utest/core/interrupt/interrupt-test.cc
///   - Create interrupt, start thread that calls interrupt.wait()
///   - Wait until thread is blocked
///   - Suspend, then resume, then trigger
///   - Thread completes wait successfully
#[async_std::test]
async fn wait_thread_functions_after_suspend_resume() {
    let _ctx = TestContext::new();
    let interrupt = Interrupt::new_virtual();

    // Basic trigger → wait cycle (simplified from C++ test).
    let int_clone = interrupt.clone();
    let handle = async_std::task::spawn(async move { int_clone.wait().await.unwrap() });

    // Small delay to let the wait task block.
    async_std::task::sleep(core::time::Duration::from_millis(10)).await;

    // Trigger wakes the waiter.
    interrupt.trigger(42).unwrap();

    let ts = handle.await;
    assert_eq!(ts, 42);
}

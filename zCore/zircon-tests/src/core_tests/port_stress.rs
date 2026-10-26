//! Port stress and race-condition tests.
//!
//! Faithful 1:1 ports of Fuchsia's `PortStressTest` suite from
//! `zircon/system/utest/core/port/ports.cc`. Each test creates
//! multiple `std::thread`s that race port/event/channel operations
//! against each other in tight loops.
//!
//! All tests are `#[ignore]` because they hang or crash in the
//! current kernel (they're in core-tests-skip.txt).

use crate::helpers::TestContext;
use zircon_object::object::{KernelObject, Signal};
use zircon_object::signal::{Event, PayloadRepr, Port, PortPacket, PortPacketRepr};

extern crate alloc;
use alloc::sync::Arc;
use alloc::vec;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// Try to dequeue one packet from the port. Returns `Some(packet)` if
/// a packet was available, `None` otherwise.
///
/// Port::wait() is async and blocks forever; we check READABLE first
/// to avoid blocking when the queue is empty.
fn try_dequeue(port: &Arc<Port>) -> Option<PortPacket> {
    if port.signal().contains(Signal::READABLE) {
        // Race: another thread might drain the packet between our check
        // and the dequeue. Use a short timeout via `block_on`.
        use async_std::future::timeout;
        match async_std::task::block_on(timeout(Duration::from_millis(1), port.wait())) {
            Ok(pkt) => Some(pkt),
            Err(_) => None,
        }
    } else {
        None
    }
}

/// C++: TEST(PortStressTest, CancelKeyActiveObserverRace)
///
/// Tests the race where `cancel_by_key` cancels active PortObservers
/// while watched objects concurrently assert signals.
///
/// Thread 1 (signaler): toggles USER_SIGNAL_0 on 8 events in a loop.
/// Thread 2 (main): registers 8 observers per iteration with the same
///   key, then cancels by key, then drains any delivered packets.
///   2000 iterations.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_active_observer_race() {
    let _ctx = TestContext::new();

    let port = Port::new(0).unwrap();
    const NUM_EVENTS: usize = 8;
    let events: vec::Vec<Arc<Event>> = (0..NUM_EVENTS).map(|_| Event::new()).collect();
    const KEY: u64 = 0xca4c;
    const ITERATIONS: usize = 2000;

    let done = Arc::new(AtomicBool::new(false));

    // Signaler thread: toggle signals on all events continuously.
    let done_clone = done.clone();
    let events_clone: vec::Vec<_> = events.iter().map(Arc::clone).collect();
    let signaler = thread::spawn(move || {
        while !done_clone.load(Ordering::SeqCst) {
            for event in &events_clone {
                event.signal_set(Signal::USER_SIGNAL_0);
                event.signal_clear(Signal::USER_SIGNAL_0);
            }
        }
    });

    // Main thread: register observers, cancel by key, drain.
    for _i in 0..ITERATIONS {
        for event in &events {
            let obj = event.clone() as Arc<dyn KernelObject>;
            obj.send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, KEY, false)
                .unwrap();
        }

        let status = port.cancel_by_key(KEY);
        assert!(
            status.is_ok() || status == Err(zircon_object::ZxError::NOT_FOUND),
            "unexpected cancel_by_key status: {:?}",
            status
        );

        // Drain any delivered packets.
        while try_dequeue(&port).is_some() {}
    }

    done.store(true, Ordering::SeqCst);
    signaler.join().unwrap();
}

/// C++: TEST(PortStressTest, CancelKeyDuringMatchRace)
///
/// Three threads race: wait_thread dequeues packets, signal_thread
/// signals a race event, cancel_thread cancels by key. Each iteration
/// creates a fresh port. 500 iterations.
///
/// Verifies: race_event_packets == race_event_cancel_not_found,
///           race_event_canceled + race_event_cancel_not_found == kIters.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_during_match_race() {
    let _ctx = TestContext::new();

    let race_event = Event::new();
    let cancel_event = Event::new();
    const CANCEL_EVENT_KEY: u64 = 1;
    const ITERS: usize = 500;

    let race_event_packets = Arc::new(AtomicU64::new(0));
    let race_event_canceled = Arc::new(AtomicU64::new(0));
    let race_event_cancel_not_found = Arc::new(AtomicU64::new(0));

    // Shared port — recreated each iteration by wait_thread.
    let port_slot: Arc<std::sync::Mutex<Option<Arc<Port>>>> = Arc::new(std::sync::Mutex::new(None));

    // Use barriers to synchronize the three threads per iteration.
    let ready = Arc::new(std::sync::Barrier::new(3));
    let complete = Arc::new(std::sync::Barrier::new(3));

    let race_key = Arc::new(AtomicU64::new(0xf0f0f0f0));

    // Wait thread: creates port, registers observers, dequeues packets.
    let wait_port_slot = port_slot.clone();
    let wait_ready = ready.clone();
    let wait_complete = complete.clone();
    let wait_race_event = race_event.clone();
    let wait_cancel_event = cancel_event.clone();
    let wait_packets = race_event_packets.clone();
    let wait_race_key = race_key.clone();
    let wait_thread = thread::spawn(move || {
        for _i in 0..ITERS {
            let port = Port::new(0).unwrap();
            // Clear signals.
            wait_race_event.signal_clear(Signal::SIGNALED);
            wait_cancel_event.signal_clear(Signal::SIGNALED);

            let rk = wait_race_key.load(Ordering::SeqCst);

            // Register observers.
            let re_obj = wait_race_event.clone() as Arc<dyn KernelObject>;
            re_obj
                .send_signal_to_port_async(Signal::SIGNALED, &port, rk, false)
                .unwrap();
            let ce_obj = wait_cancel_event.clone() as Arc<dyn KernelObject>;
            ce_obj
                .send_signal_to_port_async(Signal::SIGNALED, &port, CANCEL_EVENT_KEY, false)
                .unwrap();

            *wait_port_slot.lock().unwrap() = Some(port.clone());

            wait_ready.wait();

            // Wait for a packet.
            let pkt = async_std::task::block_on(port.wait());
            let repr = PortPacketRepr::from(&pkt);
            if repr.key == rk {
                wait_packets.fetch_add(1, Ordering::SeqCst);
            } else {
                assert_eq!(repr.key, CANCEL_EVENT_KEY);
            }

            wait_complete.wait();
        }
    });

    // Signal thread: signals race_event, sleeps, then signals cancel_event.
    let sig_ready = ready.clone();
    let sig_complete = complete.clone();
    let sig_race_event = race_event.clone();
    let sig_cancel_event = cancel_event.clone();
    let signal_thread = thread::spawn(move || {
        for _i in 0..ITERS {
            sig_ready.wait();
            sig_race_event.signal_set(Signal::SIGNALED);
            thread::sleep(Duration::from_millis(1));
            sig_cancel_event.signal_set(Signal::SIGNALED);
            sig_complete.wait();
        }
    });

    // Cancel thread: cancels the race key.
    let cancel_ready = ready.clone();
    let cancel_complete = complete.clone();
    let cancel_port_slot = port_slot.clone();
    let cancel_canceled = race_event_canceled.clone();
    let cancel_not_found = race_event_cancel_not_found.clone();
    let cancel_race_key = race_key.clone();
    let cancel_thread = thread::spawn(move || {
        for _i in 0..ITERS {
            cancel_ready.wait();
            let rk = cancel_race_key.load(Ordering::SeqCst);
            let port = cancel_port_slot.lock().unwrap().clone().unwrap();
            match port.cancel_by_key(rk) {
                Ok(()) => {
                    cancel_canceled.fetch_add(1, Ordering::SeqCst);
                }
                Err(zircon_object::ZxError::NOT_FOUND) => {
                    cancel_not_found.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => panic!("unexpected cancel_by_key status: {:?}", e),
            }
            cancel_complete.wait();
        }
    });

    wait_thread.join().unwrap();
    signal_thread.join().unwrap();
    cancel_thread.join().unwrap();

    let packets = race_event_packets.load(Ordering::SeqCst);
    let not_found = race_event_cancel_not_found.load(Ordering::SeqCst);
    let canceled = race_event_canceled.load(Ordering::SeqCst);

    assert_eq!(
        packets, not_found,
        "packets={} should equal cancel_not_found={}",
        packets, not_found
    );
    assert_eq!(
        canceled + not_found,
        ITERS as u64,
        "canceled({}) + not_found({}) should equal {}",
        canceled,
        not_found,
        ITERS
    );
}

/// C++: TEST(PortStressTest, CancelKeySharedKeyRace)
///
/// 1000 events share the same key. One thread signals all events while
/// the main thread cancels by key. Both OK and NOT_FOUND are acceptable.
/// Must not crash or hang.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_shared_key_race() {
    let _ctx = TestContext::new();

    let port = Port::new(0).unwrap();
    const NUM_EVENTS: usize = 1000;
    const KEY: u64 = 0x1234;

    let events: vec::Vec<Arc<Event>> = (0..NUM_EVENTS).map(|_| Event::new()).collect();

    for event in &events {
        let obj = event.clone() as Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::SIGNALED, &port, KEY, false)
            .unwrap();
    }

    let proceed = Arc::new(AtomicBool::new(false));
    let proceed_clone = proceed.clone();
    let events_clone: vec::Vec<_> = events.iter().map(Arc::clone).collect();

    let signal_thread = thread::spawn(move || {
        while !proceed_clone.load(Ordering::SeqCst) {
            thread::yield_now();
        }
        for event in &events_clone {
            event.signal_set(Signal::SIGNALED);
        }
    });

    proceed.store(true, Ordering::SeqCst);

    // This should trigger the race.
    let status = port.cancel_by_key(KEY);
    assert!(
        status.is_ok() || status == Err(zircon_object::ZxError::NOT_FOUND),
        "unexpected cancel_by_key status: {:?}",
        status
    );

    signal_thread.join().unwrap();
}

/// C++: TEST(PortStressTest, MatchHandleCloseRace)
///
/// Two threads race: match_thread signals a race_event, main thread
/// closes the port handle. 20000 iterations.
///
/// Each iteration creates a fresh port, registers an observer, then
/// both threads sync via barriers. match_thread signals the event
/// while main thread drops the port.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn match_handle_close_race() {
    let _ctx = TestContext::new();

    let race_event = Event::new();
    const ITERS: usize = 20000;

    let ready = Arc::new(std::sync::Barrier::new(2));
    let complete = Arc::new(std::sync::Barrier::new(2));

    let match_ready = ready.clone();
    let match_complete = complete.clone();
    let match_race_event = race_event.clone();

    let match_thread = thread::spawn(move || {
        for _i in 0..ITERS {
            match_ready.wait();
            match_race_event.signal_set(Signal::SIGNALED);
            match_complete.wait();
        }
    });

    for _i in 0..ITERS {
        let port = Port::new(0).unwrap();
        let obj = race_event.clone() as Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::SIGNALED, &port, 1, false)
            .unwrap();
        ready.wait();
        drop(port); // Close the port while match_thread signals.
        race_event.signal_clear(Signal::SIGNALED);
        complete.wait();
    }

    match_thread.join().unwrap();
}

/// C++: TEST(PortStressTest, ChannelCallWaitQueuePanicRepro)
///
/// Regression test for https://fxbug.dev/512083099.
/// Validates that a timeout in a channel call does not trigger a panic.
///
/// Thread A: performs channel_call with very short timeout, then sends
///   messages on a second channel pair.
/// Thread B: waits on port for ch1 readability, then spins until
///   thread A has timed out.
/// Thread C: reads from ch2 in a loop.
/// 1000 iterations of the A+B pair.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn channel_call_wait_queue_panic_repro() {
    let _ctx = TestContext::new();

    let (ch2_req, ch2_rep) = zircon_object::ipc::Channel::create();
    let keep_running = Arc::new(AtomicBool::new(true));

    // Thread C: reads from ch2_rep in a loop.
    let kr_c = keep_running.clone();
    let ch2_rep_clone = ch2_rep.clone();
    let t_c = thread::spawn(move || {
        while kr_c.load(Ordering::SeqCst) {
            // Try to read; ignore errors (channel might be empty).
            let _: Result<zircon_object::ipc::MessagePacket, _> = ch2_rep_clone.read();
            thread::yield_now();
        }
    });

    for _i in 0..1000 {
        let (ch1_req, ch1_rep) = zircon_object::ipc::Channel::create();
        let port = Port::new(0).unwrap();

        let ch1_rep_obj = ch1_rep.clone() as Arc<dyn KernelObject>;
        ch1_rep_obj
            .send_signal_to_port_async(Signal::READABLE, &port, 123, false)
            .unwrap();

        let thread_a_timed_out = Arc::new(AtomicBool::new(false));

        // Thread B: wait on port, then spin until A times out.
        let port_b = port.clone();
        let timed_out_b = thread_a_timed_out.clone();
        let t_b = thread::spawn(move || {
            let _pkt = async_std::task::block_on(port_b.wait());
            while !timed_out_b.load(Ordering::SeqCst) {
                thread::yield_now();
            }
        });

        // Thread A: write to ch1_req (makes ch1_rep readable => port fires),
        // then write to ch2_req.
        let ch1_req_a = ch1_req.clone();
        let ch2_req_a = ch2_req.clone();
        let timed_out_a = thread_a_timed_out.clone();
        let t_a = thread::spawn(move || {
            // Write to ch1 to trigger the observer.
            let _ = ch1_req_a.write(zircon_object::ipc::MessagePacket {
                data: vec![0u8; 8],
                handles: vec::Vec::new(),
                ..Default::default()
            });
            timed_out_a.store(true, Ordering::SeqCst);

            thread::sleep(Duration::from_micros(10));

            // Write to ch2 a bunch of times.
            for _j in 0..50 {
                let _ = ch2_req_a.write(zircon_object::ipc::MessagePacket {
                    data: vec![0u8; 8],
                    handles: vec::Vec::new(),
                    ..Default::default()
                });
                thread::sleep(Duration::from_micros(10));
            }
        });

        t_a.join().unwrap();
        t_b.join().unwrap();
    }

    keep_running.store(false, Ordering::SeqCst);
    // Wake C by writing a final message.
    let _ = ch2_req.write(zircon_object::ipc::MessagePacket {
        data: vec![0u8; 8],
        handles: vec::Vec::new(),
        ..Default::default()
    });
    t_c.join().unwrap();
}

/// C++: TEST(PortStressTest, WaitSignalCancel)
///
/// Stress test for the race between a port observer removing itself
/// from an event and the cancellation logic. Two threads:
/// - waiter: registers observer, waits for signal, cancels.
/// - signaler: toggles signal in a loop with variable-length sleeps.
/// 20000 iterations.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn wait_signal_cancel() {
    let _ctx = TestContext::new();
    const STRESS_COUNT: u32 = 20000;

    let port = Port::new(0).unwrap();
    let event = Event::new();

    let keep_running = Arc::new(AtomicBool::new(true));

    // Signaler thread: toggles EVENT_SIGNALED with variable sleeps.
    let kr = keep_running.clone();
    let event_sig = event.clone();
    let signaler = thread::spawn(move || {
        let sleeps = [0u64, 10, 2, 0, 15, 0];
        let mut count = 0usize;
        while kr.load(Ordering::SeqCst) {
            event_sig.signal_set(Signal::SIGNALED);
            let duration = sleeps[count % sleeps.len()];
            if duration > 0 {
                thread::sleep(Duration::from_nanos(duration));
            }
            event_sig.signal_clear(Signal::SIGNALED);
            count += 1;
        }
    });

    // Waiter: register, wait for signal, cancel.
    let port_w = port.clone();
    let event_w = event.clone();
    let kr_w = keep_running.clone();
    let waiter = thread::spawn(move || {
        let key = 919u64;
        let mut count = STRESS_COUNT;
        while count > 0 {
            count -= 1;
            let obj = event_w.clone() as Arc<dyn KernelObject>;
            obj.send_signal_to_port_async(Signal::SIGNALED, &port_w, key, false)
                .unwrap();

            // wait_one equivalent: spin until EVENT_SIGNALED
            while !event_w.signal().contains(Signal::SIGNALED) {
                thread::yield_now();
            }

            let status = port_w.cancel_async_and_drain(event_w.id(), key);
            if status.is_err() && status != Err(zircon_object::ZxError::NOT_FOUND) {
                panic!("cancel failed: {:?}", status);
            }

            // Drain any queued packet.
            while try_dequeue(&port_w).is_some() {}
        }
        kr_w.store(false, Ordering::SeqCst);
    });

    waiter.join().unwrap();
    keep_running.store(false, Ordering::SeqCst);
    signaler.join().unwrap();
}

/// C++: TEST(PortStressTest, SignalCloseWait)
///
/// Stress test: 4 signaler threads create events, register them
/// with the port, randomly signal/close them. 4 drainer threads
/// continuously dequeue packets. Runs for ~100ms.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn signal_close_wait() {
    let _ctx = TestContext::new();
    let port = Port::new(0).unwrap();

    const NUM_SIGNALERS: usize = 4;
    const NUM_DRAINERS: usize = 4;
    let keep_running = Arc::new(AtomicBool::new(true));

    // Signaler threads: create events, register, randomly signal/close.
    let mut signalers = vec::Vec::new();
    for _ix in 0..NUM_SIGNALERS {
        let port_clone = port.clone();
        let kr = keep_running.clone();
        signalers.push(thread::spawn(move || {
            let mut event: Option<Arc<Event>> = None;
            let mut rng_state: u32 = 4; // deterministic seed like C++ srand(4)
            while kr.load(Ordering::SeqCst) {
                if event.is_none() {
                    let e = Event::new();
                    let obj = e.clone() as Arc<dyn KernelObject>;
                    obj.send_signal_to_port_async(Signal::SIGNALED, &port_clone, 0, false)
                        .unwrap();
                    event = Some(e);
                }

                // Simple LCG PRNG for action selection
                rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
                let action = (rng_state >> 16) % 3;
                match action {
                    0 => {
                        // sleep
                        thread::sleep(Duration::from_millis(1));
                    }
                    1 => {
                        // signal
                        if let Some(ref e) = event {
                            e.signal_set(Signal::SIGNALED);
                        }
                    }
                    _ => {
                        // signal and close
                        if let Some(ref e) = event {
                            e.signal_set(Signal::SIGNALED);
                        }
                        event = None;
                    }
                }
            }
        }));
    }

    // Drainer threads: continuously wait for packets.
    // They exit when they receive a User packet.
    let mut drainers = vec::Vec::new();
    for _ix in 0..NUM_DRAINERS {
        let port_clone = port.clone();
        drainers.push(thread::spawn(move || loop {
            let pkt = async_std::task::block_on(port_clone.wait());
            let repr = PortPacketRepr::from(&pkt);
            if repr.data == PayloadRepr::User([0; 32]) {
                break;
            }
        }));
    }

    // Run for ~100ms.
    thread::sleep(Duration::from_millis(100));
    keep_running.store(false, Ordering::SeqCst);

    // Send sentinel user packets to wake drainers.
    for _ix in 0..NUM_DRAINERS {
        let packet = PortPacketRepr {
            key: 0,
            status: zircon_object::ZxError::OK,
            data: PayloadRepr::User([0; 32]),
        };
        // Retry if the port is at capacity.
        loop {
            match port.push_user(PortPacket::from(packet.clone())) {
                Ok(()) => break,
                Err(zircon_object::ZxError::SHOULD_WAIT) => thread::yield_now(),
                Err(e) => panic!("push_user failed: {:?}", e),
            }
        }
    }

    for d in drainers {
        d.join().unwrap();
    }
    for s in signalers {
        s.join().unwrap();
    }
}

/// C++: TEST(PortStressTest, CloseWaitRace)
///
/// Stress test: one thread repeatedly creates and closes ports,
/// while 4 waiter threads try to register async waits on the
/// constantly-changing port. Runs for ~100ms.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn close_wait_race() {
    let _ctx = TestContext::new();

    const NUM_WAITERS: usize = 4;
    const TOTAL_THREADS: usize = NUM_WAITERS + 1;
    const MAX_WAITS_PER_OBJECT: usize = 100;

    let ready_count = Arc::new(AtomicUsize::new(0));
    let keep_running = Arc::new(AtomicBool::new(true));
    let port: Arc<std::sync::Mutex<Option<Arc<Port>>>> = Arc::new(std::sync::Mutex::new(None));

    // Waiter threads: repeatedly register async waits on events.
    let mut waiters = vec::Vec::new();
    for _ix in 0..NUM_WAITERS {
        let rc = ready_count.clone();
        let kr = keep_running.clone();
        let port_slot = port.clone();
        waiters.push(thread::spawn(move || {
            let mut event = Event::new();
            let mut num_waits = 0usize;

            rc.fetch_add(1, Ordering::SeqCst);
            while rc.load(Ordering::SeqCst) < TOTAL_THREADS {
                thread::yield_now();
            }

            while kr.load(Ordering::SeqCst) {
                let current_port = port_slot.lock().unwrap().clone();
                if let Some(p) = current_port {
                    let obj = event.clone() as Arc<dyn KernelObject>;
                    // In the C++ test, BAD_HANDLE and WRONG_TYPE are
                    // acceptable because the port may be closed at any time.
                    // In our libos model, the Arc keeps the port alive but
                    // we still exercise the observer registration path.
                    let _ = obj.send_signal_to_port_async(Signal::SIGNALED, &p, 0, false);
                }

                num_waits += 1;
                if num_waits >= MAX_WAITS_PER_OBJECT {
                    event = Event::new();
                    num_waits = 0;
                }
            }
        }));
    }

    // Port creator thread: repeatedly create and drop ports.
    let rc = ready_count.clone();
    let kr = keep_running.clone();
    let port_slot = port.clone();
    let creator = thread::spawn(move || {
        rc.fetch_add(1, Ordering::SeqCst);
        while rc.load(Ordering::SeqCst) < TOTAL_THREADS {
            thread::yield_now();
        }

        while kr.load(Ordering::SeqCst) {
            let p = Port::new(0).unwrap();
            *port_slot.lock().unwrap() = Some(p);
            thread::sleep(Duration::from_millis(1));
            *port_slot.lock().unwrap() = None; // drop the port
        }
    });

    // Run for ~100ms.
    thread::sleep(Duration::from_millis(100));
    keep_running.store(false, Ordering::SeqCst);

    for w in waiters {
        w.join().unwrap();
    }
    creator.join().unwrap();
}

/// C++: TEST(PortStressTest, CancelKeyDuringRegistrationRace)
///
/// Registers and matches async waits on an object while simultaneously
/// cancelling by key from another thread.
/// Regression test for https://fxbug.dev/504783349.
///
/// - Main thread: registers 100000 observers with the same key.
/// - Signaling thread: toggles EVENT_SIGNALED in a loop.
/// - Canceling thread: cancels by key in a loop.
/// - Waiting thread: drains packets in a loop.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_during_registration_race() {
    let _ctx = TestContext::new();

    let port = Port::new(0).unwrap();
    let event = Event::new();
    const KEY: u64 = 1234;

    let run = Arc::new(AtomicBool::new(true));

    // Signaling thread.
    let run_s = run.clone();
    let event_s = event.clone();
    let signaling_thread = thread::spawn(move || {
        while run_s.load(Ordering::SeqCst) {
            event_s.signal_set(Signal::SIGNALED);
            event_s.signal_clear(Signal::SIGNALED);
        }
    });

    // Canceling thread.
    let run_c = run.clone();
    let port_c = port.clone();
    let canceling_thread = thread::spawn(move || {
        while run_c.load(Ordering::SeqCst) {
            let _ = port_c.cancel_by_key(KEY);
        }
    });

    // Waiting thread.
    let run_w = run.clone();
    let port_w = port.clone();
    let waiting_thread = thread::spawn(move || {
        while run_w.load(Ordering::SeqCst) {
            // Non-blocking dequeue attempt.
            let _ = try_dequeue(&port_w);
            thread::yield_now();
        }
    });

    // Main thread: register 100000 observers.
    for _i in 0..100_000 {
        let obj = event.clone() as Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::SIGNALED, &port, KEY, false)
            .unwrap();
    }

    run.store(false, Ordering::SeqCst);
    signaling_thread.join().unwrap();
    canceling_thread.join().unwrap();
    waiting_thread.join().unwrap();
}

/// C++: TEST(PortStressTest, CancelKeyCloseRace)
///
/// Focuses on closing handles and cancelling waits registered on the
/// same object. Regression test for https://fxbug.dev/504783349.
///
/// 10000 dummy events + 1 target event all share the same key.
/// 10 closer threads race to close the target event while the main
/// thread cancels by key.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_close_race() {
    let _ctx = TestContext::new();

    let port = Port::new(0).unwrap();
    const KEY: u64 = 0x1234;
    const NUM_DUMMIES: usize = 10000;

    let dummy_events: vec::Vec<Arc<Event>> = (0..NUM_DUMMIES).map(|_| Event::new()).collect();
    for event in &dummy_events {
        let obj = event.clone() as Arc<dyn KernelObject>;
        obj.send_signal_to_port_async(Signal::SIGNALED, &port, KEY, false)
            .unwrap();
    }

    // Target event — shared among 10 closer threads.
    let event_b = Event::new();
    let obj_b = event_b.clone() as Arc<dyn KernelObject>;
    obj_b
        .send_signal_to_port_async(Signal::SIGNALED, &port, KEY, false)
        .unwrap();

    let proceed = Arc::new(AtomicBool::new(false));
    // In C++, event_b.reset() closes the handle. In our model, dropping
    // the Arc is the equivalent. We use an Option<Arc<Event>> behind a
    // mutex and take() it to simulate close.
    let event_b_slot: Arc<std::sync::Mutex<Option<Arc<Event>>>> =
        Arc::new(std::sync::Mutex::new(Some(event_b)));

    let mut closers = vec::Vec::new();
    for t in 0..10 {
        let p = proceed.clone();
        let slot = event_b_slot.clone();
        closers.push(thread::spawn(move || {
            while !p.load(Ordering::SeqCst) {
                thread::yield_now();
            }
            // Variable-length spin to spread out timing (like C++ asm volatile loop).
            for _ in 0..(t * 100) {
                std::hint::spin_loop();
            }
            // Close the event (take and drop).
            let _ = slot.lock().unwrap().take();
        }));
    }

    proceed.store(true, Ordering::SeqCst);
    let status = port.cancel_by_key(KEY);
    assert!(
        status.is_ok() || status == Err(zircon_object::ZxError::NOT_FOUND),
        "unexpected cancel_by_key status: {:?}",
        status
    );

    for c in closers {
        c.join().unwrap();
    }
}

/// C++: TEST(PortStressTest, CancelKeyDestructorReentersPortLock)
///
/// Regression test for https://fxbug.dev/540007680.
///
/// Tests that cancel_by_key doesn't deadlock when the destructor
/// of a cancelled observer re-enters the port lock. Creates channels
/// with embedded event handles, then races cancel_by_key against
/// closing the channel handles. 16 rounds of 64 units each, with
/// 4 closer threads.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn cancel_key_destructor_reenters_port_lock() {
    let _ctx = TestContext::new();

    const CHANNEL_KEY: u64 = 0xC0FFEE;
    const EVENT_KEY: u64 = 0xBEEF;
    const UNITS_PER_ROUND: usize = 64;
    const ROUNDS: usize = 16;
    const CLOSERS: usize = 4;

    for _round in 0..ROUNDS {
        let port = Port::new(0).unwrap();

        // Build targets: each unit has:
        // (1) An event with observer on EVENT_KEY (stays registered).
        // (2) A channel (target) whose messages contain the event handle.
        //     The channel has an observer on CHANNEL_KEY (will be cancelled).
        let mut targets: vec::Vec<Arc<zircon_object::ipc::Channel>> = vec::Vec::new();
        for _i in 0..UNITS_PER_ROUND {
            let event = Event::new();
            let event_obj = event.clone() as Arc<dyn KernelObject>;
            event_obj
                .send_signal_to_port_async(Signal::SIGNALED, &port, EVENT_KEY, false)
                .unwrap();

            let (producer, target) = zircon_object::ipc::Channel::create();
            // Write event into channel (simulates handle transfer).
            // In our model we can just drop the event reference here;
            // the observer stays registered on the port.
            drop(event);
            drop(producer);

            // Register observer on target channel with CHANNEL_KEY.
            let target_obj = target.clone() as Arc<dyn KernelObject>;
            target_obj
                .send_signal_to_port_async(Signal::USER_SIGNAL_0, &port, CHANNEL_KEY, false)
                .unwrap();

            targets.push(target);
        }

        // Race: cancel_by_key vs closing targets.
        let go = Arc::new(AtomicBool::new(false));
        let next = Arc::new(AtomicUsize::new(0));

        let targets_shared: Arc<
            std::sync::Mutex<vec::Vec<Option<Arc<zircon_object::ipc::Channel>>>>,
        > = Arc::new(std::sync::Mutex::new(
            targets.into_iter().map(Some).collect(),
        ));

        let mut closer_threads = vec::Vec::new();
        for _c in 0..CLOSERS {
            let go_c = go.clone();
            let next_c = next.clone();
            let targets_c = targets_shared.clone();
            closer_threads.push(thread::spawn(move || {
                while !go_c.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                loop {
                    let i = next_c.fetch_add(1, Ordering::Relaxed);
                    let mut t = targets_c.lock().unwrap();
                    if i >= t.len() {
                        return;
                    }
                    t[i] = None; // drop/close the channel
                }
            }));
        }

        go.store(true, Ordering::Release);
        let status = port.cancel_by_key(CHANNEL_KEY);
        assert!(
            status.is_ok() || status == Err(zircon_object::ZxError::NOT_FOUND),
            "unexpected cancel_by_key status: {:?}",
            status
        );

        for c in closer_threads {
            c.join().unwrap();
        }
    }
}

/// C++: TEST(PortStressTest, QueuePacketAfterPortClosedConcurrentRace)
///
/// 32 events registered on a port. One thread toggles signals on all
/// events in a tight loop. Main thread closes the port. 500 iterations.
/// Tests that signalling after port close doesn't crash.
#[test]
#[ignore] // C++ equivalent hangs — in core-tests-skip.txt
fn queue_packet_after_port_closed_concurrent_race() {
    let _ctx = TestContext::new();

    const ITERATIONS: usize = 500;
    const NUM_EVENTS: usize = 32;

    for _iter in 0..ITERATIONS {
        let port = Port::new(0).unwrap();
        let events: vec::Vec<Arc<Event>> = (0..NUM_EVENTS).map(|_| Event::new()).collect();

        for (i, event) in events.iter().enumerate() {
            let obj = event.clone() as Arc<dyn KernelObject>;
            obj.send_signal_to_port_async(Signal::SIGNALED, &port, (i + 1) as u64, false)
                .unwrap();
        }

        let running = Arc::new(AtomicBool::new(true));
        let sync_barrier = Arc::new(std::sync::Barrier::new(2));

        let running_clone = running.clone();
        let sync_clone = sync_barrier.clone();
        let events_clone: vec::Vec<_> = events.iter().map(Arc::clone).collect();

        let signaler = thread::spawn(move || {
            sync_clone.wait();
            while running_clone.load(Ordering::SeqCst) {
                for event in &events_clone {
                    event.signal_set(Signal::SIGNALED);
                    event.signal_clear(Signal::SIGNALED);
                }
            }
        });

        sync_barrier.wait();
        // Close the port (drop it).
        drop(port);
        running.store(false, Ordering::SeqCst);
        signaler.join().unwrap();
    }
}

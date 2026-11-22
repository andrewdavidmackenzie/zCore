//! Time and clock functions.

use std::collections::BinaryHeap;
use std::sync::{Condvar, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// Boot instant -- captured once at startup for monotonic time.
static BOOT_INSTANT: LazyLock<Instant> = LazyLock::new(Instant::now);

/// One armed timer in the scheduler queue.
struct TimerEntry {
    deadline: Duration,
    seq: u64,
    callback: Box<dyn FnOnce(Duration) + Send + Sync>,
}

impl Eq for TimerEntry {}
impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.seq == other.seq
    }
}
impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse so BinaryHeap (a max-heap) pops the earliest deadline
        // first; the sequence number keeps same-deadline entries FIFO.
        other
            .deadline
            .cmp(&self.deadline)
            .then(other.seq.cmp(&self.seq))
    }
}

/// Shared single-thread deadline scheduler (see `scheduler()`).
struct Scheduler {
    state: Mutex<SchedState>,
    cond: Condvar,
}

struct SchedState {
    queue: BinaryHeap<TimerEntry>,
    next_seq: u64,
}

/// Get the process-wide timer scheduler, initializing it (and its single
/// worker thread) on first use.
///
/// The `Scheduler` is intentionally **leaked** (`Box::leak`) and its
/// `OnceLock` stores only a `&'static` reference: dropping either at
/// process-exit static teardown would destroy a `Mutex`/`Condvar` the
/// worker may be parked on — the exact class of post-summary crash we
/// fixed in #852. Leaked state is reclaimed by the OS at process death
/// and is safe for the worker to touch for the entire process lifetime.
fn scheduler() -> &'static Scheduler {
    static SCHED: OnceLock<&'static Scheduler> = OnceLock::new();
    SCHED.get_or_init(|| {
        let sched: &'static Scheduler = Box::leak(Box::new(Scheduler {
            state: Mutex::new(SchedState {
                queue: BinaryHeap::new(),
                next_seq: 0,
            }),
            cond: Condvar::new(),
        }));
        std::thread::Builder::new()
            .name("timer-scheduler".to_string())
            .spawn(move || scheduler_worker(sched))
            .expect("failed to spawn timer scheduler thread");
        sched
    })
}

/// Body of the single scheduler worker thread.
///
/// Drains all due entries, runs their callbacks **outside** the lock
/// (callbacks may themselves call `timer_set`, and they may take kernel
/// locks — `sched.state` must remain a leaf lock), then sleeps on the
/// condvar until the next deadline (or until a new, earlier timer is
/// pushed and notifies us).
fn scheduler_worker(sched: &'static Scheduler) {
    loop {
        let due = {
            let mut state = sched.state.lock().unwrap();
            loop {
                let now = timer_now();
                if state.queue.peek().is_some_and(|e| e.deadline <= now) {
                    let mut due = Vec::new();
                    while state.queue.peek().is_some_and(|e| e.deadline <= now) {
                        due.push(state.queue.pop().unwrap());
                    }
                    break due;
                }
                match state.queue.peek() {
                    Some(e) => {
                        let wait = e.deadline - now;
                        let (guard, _timed_out) = sched.cond.wait_timeout(state, wait).unwrap();
                        state = guard;
                    }
                    None => {
                        state = sched.cond.wait(state).unwrap();
                    }
                }
            }
        };
        let now = timer_now();
        for entry in due {
            (entry.callback)(now);
        }
    }
}

hal_fn_impl! {
    impl mod crate::hal_fn::timer {
        /// Monotonic time (duration since process start).
        fn timer_now() -> Duration {
            BOOT_INSTANT.elapsed()
        }

        /// Wall-clock time (duration since Unix epoch).
        fn timer_clock_realtime() -> Duration {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
        }

        fn timer_set(deadline: Duration, callback: Box<dyn FnOnce(Duration) + Send + Sync>) {
            // Arms the callback on the shared single-thread deadline
            // scheduler. This bounds timer threads to exactly one for
            // the whole process: previously every call spawned a
            // detached OS thread that slept until its deadline, and
            // hot paths (SleepFuture::poll, poll/epoll timeouts)
            // re-arm on every poll — thread count grew with
            // registration rate × remaining deadline (#857).
            //
            // Canceled or replaced deadlines leave a stale *queue
            // entry* (not a thread) that fires at its original
            // deadline and no-ops in the callback (Weak upgrade /
            // deadline check), which is harmless.
            //
            // Teardown safety (#852): the worker touches only the
            // leaked scheduler state and the callbacks themselves —
            // the same safety profile as the per-thread design it
            // replaces, validated over many repeated test runs.
            let sched = scheduler();
            let mut state = sched.state.lock().unwrap();
            let seq = state.next_seq;
            state.next_seq += 1;
            state.queue.push(TimerEntry {
                deadline,
                seq,
                callback,
            });
            drop(state);
            // Wake the worker so it recomputes its sleep for a
            // possibly-earlier deadline (no-op if it is busy running
            // callbacks — it re-checks the queue afterwards).
            sched.cond.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Arms many timers with staggered deadlines and requires every
    /// one to fire, then re-arms after the batch to prove the single
    /// worker stays alive and healthy (#857).
    #[test]
    fn scheduler_fires_many_timers() {
        let fired = Arc::new(AtomicUsize::new(0));
        for i in 0..200u64 {
            let fired = fired.clone();
            let deadline = timer_now() + Duration::from_millis(i % 50);
            timer_set(
                deadline,
                Box::new(move |_| {
                    fired.fetch_add(1, Ordering::SeqCst);
                }),
            );
        }
        // All deadlines fall within 50ms; allow generous slack for
        // loaded CI runners.
        let limit = timer_now() + Duration::from_millis(2000);
        while fired.load(Ordering::SeqCst) < 200 {
            assert!(
                timer_now() < limit,
                "only {} of 200 timers fired",
                fired.load(Ordering::SeqCst)
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        // Re-arm after everything has fired: the worker must still work.
        let re_fired = Arc::new(AtomicUsize::new(0));
        for _ in 0..10 {
            let re_fired = re_fired.clone();
            timer_set(
                timer_now() + Duration::from_millis(10),
                Box::new(move |_| {
                    re_fired.fetch_add(1, Ordering::SeqCst);
                }),
            );
        }
        let limit = timer_now() + Duration::from_millis(2000);
        while re_fired.load(Ordering::SeqCst) < 10 {
            assert!(timer_now() < limit, "re-armed timers did not fire");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Entries sharing a deadline must fire in the order they were
    /// armed (the sequence-number tiebreaker).
    #[test]
    fn scheduler_fifo_for_same_deadline() {
        const N: usize = 100;
        let order = Arc::new(Mutex::new(Vec::with_capacity(N)));
        let deadline = timer_now() + Duration::from_millis(20);
        for i in 0..N {
            let order = order.clone();
            timer_set(
                deadline,
                Box::new(move |_| {
                    order.lock().unwrap().push(i);
                }),
            );
        }
        let limit = timer_now() + Duration::from_millis(2000);
        {
            let mut guard = order.lock().unwrap();
            while guard.len() < N {
                drop(guard);
                assert!(timer_now() < limit, "same-deadline timers did not fire");
                std::thread::sleep(Duration::from_millis(5));
                guard = order.lock().unwrap();
            }
            let expected: Vec<usize> = (0..N).collect();
            assert_eq!(*guard, expected, "same-deadline timers must fire FIFO");
        }
    }
}

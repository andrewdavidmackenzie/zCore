//! Time and clock functions.

use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime};

/// Boot instant -- captured once at startup for monotonic time.
static BOOT_INSTANT: LazyLock<Instant> = LazyLock::new(Instant::now);

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
            // Use a plain OS thread instead of async-std's task
            // system. Async-std's executor threads outlive the test
            // binary's static teardown (~4-6% of runs crash in
            // ConcurrentQueue<TimerOp>::pop after the test summary).
            // A detached OS thread avoids that entirely: it holds no
            // references to async-std's internal structures, and the
            // process can exit while it sleeps without heap
            // corruption (the OS reclaims everything).
            let set_at = timer_now();
            std::thread::spawn(move || {
                let thread_start = timer_now();
                if deadline > thread_start {
                    std::thread::sleep(deadline - thread_start);
                }
                let fired_at = timer_now();
                if fired_at - set_at > deadline - set_at + Duration::from_millis(5) {
                    eprintln!(
                        "[timer] set_at={:?} start_at={:?} deadline={:?} fired_at={:?} (spawn_delay={:?}, total_delay={:?}, overshoot={:?})",
                        set_at, thread_start, deadline, fired_at,
                        thread_start - set_at,
                        fired_at - set_at,
                        fired_at.saturating_sub(deadline)
                    );
                }
                callback(fired_at);
            });
        }
    }
}

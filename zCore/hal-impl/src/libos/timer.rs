//! Time and clock functions.

use async_std::task;
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
            task::spawn(async move {
                let dur = deadline - timer_now();
                task::sleep(dur).await;
                callback(timer_now());
            });
        }
    }
}

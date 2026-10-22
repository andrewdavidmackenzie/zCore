//! Time and clock functions.

use alloc::boxed::Box;
use core::time::Duration;

use lock::Mutex;
use naive_timer::Timer;

/// Timer tick rate. 100 Hz matches the default Linux HZ on most configs.
/// Higher values give better interactive responsiveness at the cost of
/// more timer interrupt overhead (especially severe in QEMU).
pub(super) const TICKS_PER_SEC: u64 = 100;

static NAIVE_TIMER: spin::Lazy<Mutex<Timer>> = spin::Lazy::new(|| Mutex::new(Timer::default()));

hal_fn_impl! {
    impl mod crate::hal_fn::timer {
        fn timer_enable() {
            super::arch::timer_init();
        }

        fn timer_now() -> Duration {
            super::arch::timer::timer_now()
        }

        fn timer_set(deadline: Duration, callback: Box<dyn FnOnce(Duration) + Send + Sync>) {
            debug!("Set timer at: {:?}", deadline);
            NAIVE_TIMER.lock().add(deadline, callback);
            // If the deadline is already past (or very near), expire
            // immediately without waiting for the next periodic tick.
            // This gives precise deadline handling for short timeouts
            // (e.g., 1us channel_call deadlines) while keeping the
            // periodic 100 Hz tick for scheduler preemption.
            let now = super::arch::timer::timer_now();
            if deadline <= now {
                NAIVE_TIMER.lock().expire(now);
            }
        }

        fn timer_tick() {
            NAIVE_TIMER.lock().expire(timer_now());
        }
    }
}

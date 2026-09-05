//! Time and clock functions.

use alloc::boxed::Box;
use core::time::Duration;

use lock::Mutex;
use naive_timer::Timer;

/// Timer tick rate. 100 Hz matches the default Linux HZ on most configs.
/// Higher values give better interactive responsiveness at the cost of
/// more timer interrupt overhead.
pub(super) const TICKS_PER_SEC: u64 = 100;

lazy_static::lazy_static! {
    static ref NAIVE_TIMER:Mutex<Timer> = Mutex::new(Timer::default());
}

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
        }

        fn timer_tick() {
            NAIVE_TIMER.lock().expire(timer_now());
        }
    }
}

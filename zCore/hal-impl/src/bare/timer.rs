//! Time and clock functions — one-shot timer model.
//!
//! Instead of a periodic tick, each `timer_set` reprograms the hardware
//! timer to fire at the earliest pending deadline (capped at 10ms for
//! scheduler preemption). This matches Fuchsia's `platform_set_oneshot_timer`
//! model and provides microsecond-precision deadline handling.

use alloc::boxed::Box;
use core::time::Duration;

use lock::Mutex;
use naive_timer::Timer;

/// Maximum interval between timer interrupts (backstop for scheduler
/// preemption). Even when no software timers are pending, we ensure
/// the hardware fires at least every 10ms so the executor can
/// re-evaluate task priorities.
const MAX_TIMER_INTERVAL: Duration = Duration::from_millis(10);

static NAIVE_TIMER: spin::Lazy<Mutex<Timer>> = spin::Lazy::new(|| Mutex::new(Timer::default()));

/// Reprogram the hardware timer to fire at the earliest pending
/// deadline, capped at `now + MAX_TIMER_INTERVAL`. If the next
/// deadline is already past, expire it synchronously to avoid
/// interrupt re-entry (a past deadline would fire the hardware
/// timer immediately, potentially nesting into the current handler
/// and overflowing the interrupt stack).
///
/// Caller must NOT hold NAIVE_TIMER lock.
fn update_platform_timer() {
    loop {
        let now = super::arch::timer::timer_now();
        let next = NAIVE_TIMER.lock().next();

        match next {
            Some(deadline) if deadline <= now => {
                // Already past — expire synchronously, then check
                // if more timers became ready.
                NAIVE_TIMER.lock().expire(now);
                continue;
            }
            Some(deadline) => {
                let backstop = now + MAX_TIMER_INTERVAL;
                super::arch::timer::set_oneshot_timer(deadline.min(backstop));
            }
            None => {
                let backstop = now + MAX_TIMER_INTERVAL;
                super::arch::timer::set_oneshot_timer(backstop);
            }
        }
        break;
    }
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
            // If the deadline is already past, expire immediately.
            let now = super::arch::timer::timer_now();
            if deadline <= now {
                NAIVE_TIMER.lock().expire(now);
            }
            // Reprogram hardware to fire at the earliest deadline.
            // This is the key difference from the old periodic model:
            // a 1us deadline will now fire in ~1us instead of waiting
            // up to 10ms for the next tick.
            update_platform_timer();
        }

        fn timer_tick() {
            NAIVE_TIMER.lock().expire(timer_now());
            // Reprogram this CPU's timer to the next pending deadline
            // (capped at backstop). We call this from the interrupt
            // handler, so the timer is currently disarmed (one-shot
            // already fired). We must re-arm to ensure future timers
            // and preemption ticks continue to fire.
            let now = super::arch::timer::timer_now();
            let backstop = now + MAX_TIMER_INTERVAL;
            let next = NAIVE_TIMER.lock().next().unwrap_or(backstop);
            super::arch::timer::set_oneshot_timer(next.min(backstop));
        }
    }
}

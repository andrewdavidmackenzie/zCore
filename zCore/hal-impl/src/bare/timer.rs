//! Time and clock functions — one-shot timer model.
//!
//! Matches Fuchsia's timer architecture: a global timer queue protected
//! by a spinlock with interrupts disabled. Each `timer_set` adds a
//! callback and reprograms the local APIC/timer to fire at the earliest
//! pending deadline. The timer interrupt handler expires due timers and
//! reprograms to the next deadline.
//!
//! All timer queue access and hardware programming happen inside a
//! single critical section (interrupts off + lock held), preventing
//! races between task-context `timer_set` and interrupt-context
//! `timer_tick` on the same CPU. This matches Fuchsia's
//! `Guard<MonitoredSpinLock, IrqSave>` pattern.

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

/// Program this CPU's hardware timer to the next pending deadline,
/// capped at `now + MAX_TIMER_INTERVAL`.
///
/// MUST be called with NAIVE_TIMER lock held and interrupts disabled.
/// This matches Fuchsia's `UpdatePlatformTimerLocked` which asserts
/// `arch_ints_disabled()` and runs under `timer_lock`.
fn update_platform_timer_locked(timer: &Timer) {
    let now = super::arch::timer::timer_now();
    let backstop = now + MAX_TIMER_INTERVAL;
    let next = timer.next().unwrap_or(backstop);
    let target = next.min(backstop);
    super::arch::timer::set_oneshot_timer(target);
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

            // Disable interrupts first, then acquire the lock.
            // This prevents the timer interrupt from firing while we
            // hold the lock (same-CPU deadlock) and ensures the APIC
            // programming is atomic w.r.t. interrupts (no immediate
            // re-entry). Matches Fuchsia's IrqSave guard pattern.
            let irq_was_on = crate::interrupt::intr_get();
            crate::interrupt::intr_off();
            {
                let mut timer = NAIVE_TIMER.lock();
                timer.add(deadline, callback);
                // Expire past-due timers while we hold the lock.
                let now = super::arch::timer::timer_now();
                if deadline <= now {
                    timer.expire(now);
                }
                // Reprogram hardware under the lock with interrupts off.
                update_platform_timer_locked(&timer);
            }
            if irq_was_on {
                crate::interrupt::intr_on();
            }
        }

        fn timer_tick() {
            // Called from the timer interrupt handler.
            // Interrupts are already disabled by the interrupt gate.
            let now = super::arch::timer::timer_now();
            let mut timer = NAIVE_TIMER.lock();
            timer.expire(now);
            // Re-arm to the next deadline under the lock.
            update_platform_timer_locked(&timer);

        }
    }
}

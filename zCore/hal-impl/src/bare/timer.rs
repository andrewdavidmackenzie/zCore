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
//!
//! Timer callbacks (e.g., `signal_set`) acquire `KObjectBase::inner`
//! which is an InterruptSpinLock, so they are safe to call from IRQ
//! context. Other kernel object locks (KernelMutex) must NOT be
//! acquired from timer callbacks.

use alloc::boxed::Box;
use core::time::Duration;

use lock::IrqLock;
use naive_timer::Timer;

/// Maximum interval between timer interrupts (backstop for scheduler
/// preemption). Even when no software timers are pending, we ensure
/// the hardware fires at least every 10ms so the executor can
/// re-evaluate task priorities.
const MAX_TIMER_INTERVAL: Duration = Duration::from_millis(10);

static NAIVE_TIMER: spin::Lazy<IrqLock<Timer>> = spin::Lazy::new(|| IrqLock::new(Timer::default()));

/// #862 diagnostics: timer activity counters.
static TICK_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static EXPIRE_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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
                // #862 diagnostics: count executed callbacks.
                timer.add(deadline, move |now| {
                    EXPIRE_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    callback(now);
                });
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
            let before = timer.next();
            timer.expire(now);
            // Re-arm to the next deadline under the lock.
            update_platform_timer_locked(&timer);

            // #862 diagnostics: every 10k ticks, report rates and the
            // gap between now and the deadline we had armed. A tick
            // storm with no expirations = the hardware fires before the
            // deadline (early-fire loop); expirations growing with
            // ticks = callbacks feeding a wake storm.
            use core::sync::atomic::Ordering;
            let n = TICK_COUNT.fetch_add(1, Ordering::Relaxed);
            if n > 0 && n % 10_000 == 0 {
                let expired = EXPIRE_COUNT.load(Ordering::Relaxed);
                let gap_us = before
                    .map(|d| d.as_nanos().saturating_sub(now.as_nanos()) / 1000)
                    .unwrap_or(999_999_999);
                warn!(
                    "[timer] ticks={} expired={} armed_gap={}us now={:?}",
                    n, expired, gap_us, now
                );
            }
        }
    }
}

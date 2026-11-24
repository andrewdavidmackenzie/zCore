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
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::time::Duration;

use lock::IrqLock;
use naive_timer::Timer;

/// Maximum interval between timer interrupts (backstop for scheduler
/// preemption). Even when no software timers are pending, we ensure
/// the hardware fires at least every 10ms so the executor can
/// re-evaluate task priorities.
const MAX_TIMER_INTERVAL: Duration = Duration::from_millis(10);

static NAIVE_TIMER: spin::Lazy<IrqLock<Timer>> = spin::Lazy::new(|| IrqLock::new(Timer::default()));

/// Deferred timer callbacks (#523/#829 fix).
///
/// `Timer::expire` runs callbacks INLINE while the `NAIVE_TIMER`
/// IrqLock is held with interrupts disabled. The Zircon timer object's
/// callback (`TimerSignal::touch`) fires a signal and wakes waiters —
/// kernel-object work that acquires `KernelMutex`es (ports, thread
/// state, signal handlers) — a genuine spinlock→KernelMutex nesting
/// violation that escalates to the mutex.rs nested-spin panic whenever
/// the wakeup contends with another CPU (constant during
/// process-crash teardown: every CPU's timer fires into the cascade).
///
/// Fix, matching Fuchsia's timer-DPC design: timer callbacks may only
/// do interrupt-safe, lock-free work — deferring the heavy part here.
/// The callback (running under the timer lock) enqueues an `Arc`'d
/// closure into this queue (a nested but briefly-held IrqLock, no
/// allocation); the queue is drained AFTER the timer lock is released,
/// where kernel-object locks may be taken and yielding is possible.
/// The type of deferred timer callbacks: heavy work moved out of the
/// timer interrupt context (see below).
pub type DeferredCallback = Arc<dyn Fn(Duration) + Send + Sync>;

static DEFERRED_CALLBACKS: IrqLock<Vec<(DeferredCallback, Duration)>> = IrqLock::new(Vec::new());

/// Enqueue heavy work from a timer callback. Interrupt-safe,
/// allocation-free; must be called while `NAIVE_TIMER` is held (or any
/// interrupt context).
pub fn enqueue_deferred_timer_callback(f: Arc<dyn Fn(Duration) + Send + Sync>, now: Duration) {
    DEFERRED_CALLBACKS.lock().push((f, now));
}

/// Drain deferred timer callbacks. MUST run in task context: the
/// callbacks perform kernel-object work (signals, waiters) that may
/// contend KernelMutexes and yield. Returns the number of callbacks
/// executed (callers use it to choose between yield and sleep).
fn drain_deferred_callbacks() -> usize {
    let batch = core::mem::take(&mut *DEFERRED_CALLBACKS.lock());
    let n = batch.len();
    for (f, now) in batch {
        f(now);
    }
    n
}

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
    // #862 diagnostics: the deadline actually programmed into the
    // hardware (min of queue head and backstop), for gap reporting.
    LAST_ARMED_NANOS.store(
        target.as_nanos() as u64,
        core::sync::atomic::Ordering::Relaxed,
    );
    super::arch::timer::set_oneshot_timer(target);
}

/// #862 diagnostics: the last deadline programmed into the hardware.
static LAST_ARMED_NANOS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

hal_fn_impl! {
    impl mod crate::hal_fn::timer {
        fn timer_enable() {
            super::arch::timer_init();
        }

        fn timer_now() -> Duration {
            super::arch::timer::timer_now()
        }

        fn defer_timer_callback(f: DeferredCallback, now: Duration) {
            enqueue_deferred_timer_callback(f, now)
        }

        fn drain_deferred_timer_callbacks() -> usize {
            drain_deferred_callbacks()
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
            {
                let mut timer = NAIVE_TIMER.lock();
                timer.expire(now);
                // Re-arm to the next deadline under the lock.
                update_platform_timer_locked(&timer);
            }
            // NOTE: deferred callbacks are NOT drained here — running
            // kernel-object work from IRQ context re-introduces the
            // #862 hazard and nests IRQs (found the hard way: early
            // boot hung). A dedicated task-context drain task owns
            // them; see drain_deferred_timer_callbacks.
            // #862 diagnostics: report OUTSIDE the NAIVE_TIMER lock —
            // the logger allocates and may contend the heap lock, and
            // logging under the timer lock can deadlock against
            // timer_set on another CPU. Every 10k ticks: rates and the
            // gap between now and the deadline actually armed.
            use core::sync::atomic::Ordering;
            let n = TICK_COUNT.fetch_add(1, Ordering::Relaxed);
            if n > 0 && n.is_multiple_of(10_000) {
                let expired = EXPIRE_COUNT.load(Ordering::Relaxed);
                let armed = LAST_ARMED_NANOS.load(Ordering::Relaxed);
                let gap_us = armed.saturating_sub(now.as_nanos() as u64) / 1000;
                warn!(
                    "[timer] ticks={} expired={} armed_gap={}us now={:?}",
                    n, expired, gap_us, now
                );
            }
        }
    }
}

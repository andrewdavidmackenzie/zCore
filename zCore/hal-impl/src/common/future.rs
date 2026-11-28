use ::drivers::scheme::DisplayScheme;
use alloc::{boxed::Box, sync::Arc};
use core::task::{Context, Poll};
use core::time::Duration;
use core::{future::Future, pin::Pin};

use crate::timer;

#[must_use = "`yield_now()` does nothing unless polled/`await`-ed"]
#[derive(Default)]
pub(super) struct YieldFuture {
    flag: bool,
}

impl Future for YieldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if self.flag {
            Poll::Ready(())
        } else {
            self.flag = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Like YieldFuture but uses the YIELD_PENDING flag instead of
/// self-waking. The executor defers re-queueing this task until
/// after scanning for new notifications, ensuring newly-spawned
/// tasks get priority.
#[must_use = "`preempt_yield()` does nothing unless polled/`await`-ed"]
#[derive(Default)]
pub(super) struct PreemptYieldFuture {
    flag: bool,
}

impl Future for PreemptYieldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        if self.flag {
            Poll::Ready(())
        } else {
            self.flag = true;
            #[cfg(not(feature = "libos"))]
            executor::set_yield_pending();
            #[cfg(feature = "libos")]
            _cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[must_use = "`sleep_until()` does nothing unless polled/`await`-ed"]
pub struct SleepFuture {
    deadline: Duration,
}

impl SleepFuture {
    pub fn new(deadline: Duration) -> Self {
        Self { deadline }
    }
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if timer::timer_now() >= self.deadline {
            return Poll::Ready(());
        }
        if self.deadline.as_nanos() < i64::MAX as u128 {
            let waker = cx.waker().clone();
            // #523/#829: `Timer::expire` runs this callback in IRQ
            // context (timer_tick, under NAIVE_TIMER and the APIC
            // driver's manager_lapic IrqLocks). `wake_by_ref` takes
            // the WakerTarget's inner `spin::Mutex` and sends an IPI —
            // and a task-context holder of that mutex (notify →
            // ipi_wakeup → send_ipi) can be interrupted by the timer
            // IRQ and then spin on manager_lapic: the ABBA deadlock
            // behind the core-tests system freeze (all 8 CPUs in
            // Apic::handle_irq, caught live via gdb). Timer callbacks
            // must NEVER wake directly — defer the entire wake to the
            // drain task (task context, where blocking on the waker
            // mutex is safe). This mirrors Zircon's timer/DPC split.
            timer::timer_set(
                self.deadline,
                Box::new(move |now| {
                    crate::timer::defer_timer_callback(
                        Arc::new(move |_| {
                            waker.wake_by_ref();
                        }),
                        now,
                    );
                }),
            );
            // Re-check after registering the timer. For very short
            // deadlines (e.g., 1 usec), the deadline may have passed
            // during the timer_set call. Without this check, we'd
            // return Pending and wait up to 10ms for the next timer
            // tick, even though the deadline already expired.
            if timer::timer_now() >= self.deadline {
                cx.waker().wake_by_ref();
            }
        }
        Poll::Pending
    }
}

/// Future that reads from the shared console input buffer.
///
/// Completes when at least one byte is available from any input device
/// (UART, PS/2 keyboard, etc.) that has pushed data into `ConsoleInput`.
#[must_use = "`console_read()` does nothing unless polled/`await`-ed"]
pub(super) struct ConsoleReadFuture<'a> {
    buf: &'a mut [u8],
}

impl<'a> ConsoleReadFuture<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf }
    }
}

impl Future for ConsoleReadFuture<'_> {
    type Output = usize;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let buf = &mut self.get_mut().buf;
        let n = super::console::console_input_poll(buf, Some(cx.waker().clone()));
        if n > 0 {
            Poll::Ready(n)
        } else {
            Poll::Pending
        }
    }
}

pub(crate) struct DisplayFlushFuture {
    next_flush_time: Duration,
    frame_time: Duration,
    display: Arc<dyn DisplayScheme>,
}

impl DisplayFlushFuture {
    #[allow(dead_code)]
    pub fn new(display: Arc<dyn DisplayScheme>, refresh_rate: usize) -> Self {
        Self {
            next_flush_time: Duration::default(),
            frame_time: Duration::from_millis(1000 / refresh_rate as u64),
            display,
        }
    }
}

impl Future for DisplayFlushFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let now = timer::timer_now();
        if now >= self.next_flush_time {
            self.display.flush().ok();
            let frame_time = self.frame_time;
            self.next_flush_time += frame_time;
            // Same #523/#829 pattern as SleepFuture: defer the
            // entire wake out of the timer/IRQ context (see there).
            let waker = cx.waker().clone();
            timer::timer_set(
                self.next_flush_time,
                Box::new(move |now| {
                    crate::timer::defer_timer_callback(
                        Arc::new(move |_| {
                            waker.wake_by_ref();
                        }),
                        now,
                    );
                }),
            );
        }
        Poll::Pending
    }
}

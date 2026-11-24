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
            let waker2 = cx.waker().clone();
            // #523/#829: `Timer::expire` calls this callback and then
            // DROPS its box while the NAIVE_TIMER IrqLock is held with
            // interrupts disabled. `wake_by_ref` itself is lock-free
            // (an atomic notification) and safe there — but the box's
            // captured Waker may be the LAST reference to a completed
            // task's waker machinery, and dropping it cascades into
            // task/future teardown (`CurrentThread::drop →
            // Process::terminate → VMAR locks`): KernelMutex work that
            // cannot yield under the timer spinlock — the residual
            // mutex.rs nested-spin panic during crash teardowns
            // (caught live via the lock_anomaly_detected breakpoint).
            // So: wake inline (no added latency), but defer the
            // potentially-heavy drop to the drain task.
            timer::timer_set(
                self.deadline,
                Box::new(move |now| {
                    waker.wake_by_ref();
                    crate::timer::defer_timer_callback(
                        // The closure owns waker2; it drops when the
                        // drain task drops this Arc — never inside the
                        // timer spinlock.
                        Arc::new(move |_| {
                            let _keep_alive = &waker2;
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
            // Same #523/#829 pattern as SleepFuture: wake inline
            // (lock-free), defer the Waker drop out of the timer
            // spinlock — it may be the task's last reference.
            let waker = cx.waker().clone();
            let waker2 = cx.waker().clone();
            timer::timer_set(
                self.next_flush_time,
                Box::new(move |now| {
                    waker.wake_by_ref();
                    crate::timer::defer_timer_callback(
                        // The closure owns waker2; it drops when the
                        // drain task drops this Arc — never inside the
                        // timer spinlock.
                        Arc::new(move |_| {
                            let _keep_alive = &waker2;
                        }),
                        now,
                    );
                }),
            );
        }
        Poll::Pending
    }
}

use alloc::{boxed::Box, sync::Arc};
use core::task::{Context, Poll};
use core::time::Duration;
use core::{future::Future, pin::Pin};
use kernel_drivers::scheme::DisplayScheme;

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
            timer::timer_set(self.deadline, Box::new(move |_| waker.wake_by_ref()));
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
            let waker = cx.waker().clone();
            timer::timer_set(self.next_flush_time, Box::new(move |_| waker.wake_by_ref()));
        }
        Poll::Pending
    }
}

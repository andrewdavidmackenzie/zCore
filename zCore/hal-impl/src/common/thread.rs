use core::time::Duration;

pub use super::future::SleepFuture;
use super::future::YieldFuture;

/// Sleeps until the specified of time.
pub async fn sleep_until(deadline: Duration) {
    SleepFuture::new(deadline).await
}

/// Yields execution back to the async runtime.
pub async fn yield_now() {
    YieldFuture::default().await
}

/// Preemptive yield: yields execution and ensures the current task
/// goes to the BACK of the scheduler queue, after any newly-notified
/// tasks. Used by the timer interrupt handler to give child threads
/// a fair chance to run.
///
/// Unlike `yield_now()` (which self-wakes via the waker, potentially
/// getting re-polled immediately), this sets a flag that the executor
/// uses to defer re-queueing until after scanning for new notifications.
pub async fn preempt_yield() {
    super::future::PreemptYieldFuture::default().await
}

/// Perform an executor-level context switch for preemptive scheduling.
#[cfg(not(feature = "libos"))]
pub fn sched_yield() {
    executor::sched_yield();
}

#[cfg(feature = "libos")]
pub fn sched_yield() {
    // No-op for libos.
}

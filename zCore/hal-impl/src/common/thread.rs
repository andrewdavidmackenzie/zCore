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

/// Perform an executor-level context switch for preemptive scheduling.
#[cfg(not(feature = "libos"))]
pub fn sched_yield() {
    executor::sched_yield();
}

#[cfg(feature = "libos")]
pub fn sched_yield() {
    // No-op for libos.
}

//! Test runtime abstraction layer.
//!
//! In libos mode (`cfg(test)`), delegates to `async_std`.
//! In bare-metal mode (`feature = "baremetal-test"`), uses the kernel
//! executor primitives.

// ── libos mode: delegate to async_std ──────────────────────────────

#[cfg(test)]
pub fn block_on<F: core::future::Future>(future: F) -> F::Output {
    async_std::task::block_on(future)
}

#[cfg(test)]
pub async fn yield_now() {
    async_std::task::yield_now().await;
}

#[cfg(test)]
pub async fn sleep(duration: core::time::Duration) {
    async_std::task::sleep(duration).await;
}

#[cfg(test)]
pub fn spawn<F>(future: F) -> async_std::task::JoinHandle<F::Output>
where
    F: core::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    async_std::task::spawn(future)
}

#[cfg(test)]
pub async fn timeout<F: core::future::Future>(
    duration: core::time::Duration,
    future: F,
) -> Result<F::Output, async_std::future::TimeoutError> {
    async_std::future::timeout(duration, future).await
}

// ── bare-metal mode: kernel executor ───────────────────────────────

#[cfg(not(test))]
pub async fn yield_now() {
    // On bare-metal, yield to the kernel executor so other tasks
    // (Zircon threads) can make progress.
    core::future::poll_fn(|cx: &mut core::task::Context<'_>| -> core::task::Poll<()> {
        cx.waker().wake_by_ref();
        core::task::Poll::Pending
    })
    .await;
}

#[cfg(not(test))]
pub async fn sleep(_duration: core::time::Duration) {
    // TODO: implement timer-based sleep using HAL timer.
    // For now, just yield once.
    yield_now().await;
}

/// Spawn a task — bare-metal version.
/// Returns a JoinHandle that completes when the task finishes.
#[cfg(not(test))]
pub fn spawn<F>(future: F) -> BaremetalJoinHandle<F::Output>
where
    F: core::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};

    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let result: Arc<spin::Mutex<Option<F::Output>>> = Arc::new(spin::Mutex::new(None));
    let result_clone = result.clone();

    executor::spawn(async move {
        let val = future.await;
        *result_clone.lock() = Some(val);
        done_clone.store(true, Ordering::Release);
    });

    BaremetalJoinHandle { done, result }
}

/// A join handle for bare-metal spawned tasks.
#[cfg(not(test))]
pub struct BaremetalJoinHandle<T> {
    done: alloc::sync::Arc<core::sync::atomic::AtomicBool>,
    result: alloc::sync::Arc<spin::Mutex<Option<T>>>,
}

#[cfg(not(test))]
impl<T> core::future::Future for BaremetalJoinHandle<T> {
    type Output = T;
    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<T> {
        if self.done.load(core::sync::atomic::Ordering::Acquire) {
            let val = self
                .result
                .lock()
                .take()
                .expect("join handle polled after completion");
            core::task::Poll::Ready(val)
        } else {
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

/// Timeout wrapper — bare-metal version.
#[cfg(not(test))]
pub async fn timeout<F: core::future::Future>(
    _duration: core::time::Duration,
    future: F,
) -> Result<F::Output, TimeoutError> {
    // TODO: implement real timeout with HAL timer.
    Ok(future.await)
}

/// Timeout error type for bare-metal mode.
#[cfg(not(test))]
#[derive(Debug)]
pub struct TimeoutError;

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
    core::future::poll_fn(|cx| {
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

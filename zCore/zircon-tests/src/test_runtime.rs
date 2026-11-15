//! Test runtime abstraction layer.
//!
//! Provides `block_on`, `yield_now`, `sleep`, and `spawn` functions
//! that delegate to `async_std` in libos mode and will use the kernel
//! executor in bare-metal (QEMU) mode.
//!
//! All test code should use `crate::test_runtime::*` instead of
//! `async_std::task::*` directly, enabling future bare-metal execution.

/// Run a future to completion synchronously.
pub fn block_on<F: core::future::Future>(future: F) -> F::Output {
    async_std::task::block_on(future)
}

/// Yield to other tasks cooperatively.
pub async fn yield_now() {
    async_std::task::yield_now().await;
}

/// Sleep for the given duration.
pub async fn sleep(duration: core::time::Duration) {
    async_std::task::sleep(duration).await;
}

/// Spawn a task on the executor.
pub fn spawn<F>(future: F) -> async_std::task::JoinHandle<F::Output>
where
    F: core::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    async_std::task::spawn(future)
}

/// Timeout wrapper for futures.
pub async fn timeout<F: core::future::Future>(
    duration: core::time::Duration,
    future: F,
) -> Result<F::Output, async_std::future::TimeoutError> {
    async_std::future::timeout(duration, future).await
}

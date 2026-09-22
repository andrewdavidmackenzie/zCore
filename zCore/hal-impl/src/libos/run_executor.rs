//! LibOS executor: runs a future to completion using async-std.

use core::future::Future;

/// Run a future to completion on the host OS thread pool.
/// Returns the exit code from the future.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    async_std::task::block_on(future)
}

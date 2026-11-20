//! Bare-metal executor: runs futures via the custom executor with
//! interrupt-driven scheduling.

use core::future::Future;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
/// Whether the wrapped future has completed. The run loop returns
/// based on THIS flag, not on the local CPU's idleness: the future is
/// spawned via `spawn_with_priority`, which may place it on another
/// CPU, and any task that stays parked on this CPU (e.g. a thread
/// whose kill wake-up was lost) would otherwise keep the loop waiting
/// forever after the future completed — hanging shutdown (issue
/// #834). The flag also prevents returning a stale EXIT_CODE before
/// the future has run at all.
static EXITED: AtomicBool = AtomicBool::new(false);

/// Run the executor loop until the wrapped future completes.
///
/// Enables the timer and interrupts, then loops running the executor
/// until the future completes, sleeping between iterations via
/// wait-for-interrupt.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    // Spawn at the same priority as Zircon threads (16) so the
    // test runner isn't starved by spawned thread tasks.
    executor::spawn_with_priority(
        async move {
            let code = future.await;
            EXIT_CODE.store(code, Ordering::Relaxed);
            EXITED.store(true, Ordering::Release);
            // Regain control on every CPU: run_until_idle loops would
            // otherwise spin inside their executors' idle hlt while
            // parked tasks remain (issue #834 zombie tasks).
            executor::request_shutdown();
        },
        16,
    );

    crate::timer::timer_enable();
    log::info!("executor run!");
    crate::interrupt::intr_on();

    loop {
        executor::run_until_idle();
        if EXITED.load(Ordering::Acquire) {
            return EXIT_CODE.load(Ordering::Relaxed);
        }
        crate::interrupt::wait_for_interrupt();
    }
}

//! Bare-metal executor: runs futures via the custom executor with
//! interrupt-driven scheduling.

use core::future::Future;
use core::sync::atomic::{AtomicI32, Ordering};

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Run the executor loop until all tasks complete.
///
/// Enables the timer and interrupts, then loops running the executor
/// until idle, sleeping between iterations via wait-for-interrupt.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    // Spawn at the same priority as Zircon threads (16) so the
    // test runner isn't starved by spawned thread tasks.
    executor::spawn_with_priority(
        async move {
            let code = future.await;
            EXIT_CODE.store(code, Ordering::Relaxed);
        },
        16,
    );

    crate::timer::timer_enable();
    log::info!("executor run!");
    crate::interrupt::intr_on();

    loop {
        let has_task = executor::run_until_idle();
        if !has_task {
            return EXIT_CODE.load(Ordering::Relaxed);
        }
        crate::interrupt::wait_for_interrupt();
    }
}
